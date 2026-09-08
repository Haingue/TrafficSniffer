use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use chrono::Utc;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::Incoming;
use hyper::{Request, Response, StatusCode, Uri};

use crate::domain::config::ProxyConfig;
use crate::domain::traffic::TrafficRecord;
use crate::infrastructure::metrics::Metrics;
use crate::infrastructure::tls::ProxyClient;
use crate::infrastructure::traffic_log::TrafficLogger;

type ProxyBody = BoxBody<Bytes, hyper::Error>;
pub type ProxyResponse = Response<ProxyBody>;

#[derive(Clone)]
pub struct ProxyService {
    client: ProxyClient,
    config: Arc<ProxyConfig>,
    logger: TrafficLogger,
    metrics: Arc<Metrics>,
}

impl ProxyService {
    pub fn new(
        client: ProxyClient,
        config: Arc<ProxyConfig>,
        logger: TrafficLogger,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            client,
            config,
            logger,
            metrics,
        }
    }

    pub async fn forward_request(
        &self,
        request: Request<Incoming>,
        remote_addr: SocketAddr,
    ) -> Result<ProxyResponse, Infallible> {
        let start = Instant::now();
        let record_context = RequestContext::from_request(&request, remote_addr);
        self.metrics
            .record_request(&record_context.path, &record_context.client_ip);
        let target_uri = self.build_target_uri(&request);
        let (mut request_parts, body) = request.into_parts();
        request_parts.uri = target_uri;

        let response = self
            .client
            .request(Request::from_parts(request_parts, body))
            .await;
        let status_code = response
            .as_ref()
            .map(|value| value.status().as_u16())
            .unwrap_or(StatusCode::BAD_GATEWAY.as_u16());

        self.logger.record(&TrafficRecord {
            timestamp: Utc::now().to_rfc3339(),
            client_ip: record_context.client_ip,
            method: record_context.method,
            host: record_context.host,
            path: record_context.path,
            query: record_context.query,
            listen_port: self.config.listen_port.clone(),
            target_host: self.config.target_host.clone(),
            target_port: self.config.target_port.clone(),
            user_agent: record_context.user_agent,
            referer: record_context.referer,
            status_code,
            duration_ms: start.elapsed().as_millis(),
        });

        match response {
            Ok(response) => Ok(response.map(|body| body.boxed())),
            Err(error) => {
                eprintln!("proxy error: {}", error);
                Ok(Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(boxed_body("Bad Gateway"))
                    .unwrap())
            }
        }
    }

    fn build_target_uri(&self, request: &Request<Incoming>) -> Uri {
        let mut parts = request.uri().clone().into_parts();
        parts.scheme = self.config.target_uri.scheme().cloned();
        parts.authority = self.config.target_uri.authority().cloned();
        Uri::from_parts(parts).unwrap_or_else(|_| self.config.target_uri.clone())
    }
}

struct RequestContext {
    client_ip: String,
    method: String,
    host: String,
    path: String,
    query: Option<String>,
    user_agent: Option<String>,
    referer: Option<String>,
}

impl RequestContext {
    fn from_request(request: &Request<Incoming>, remote_addr: SocketAddr) -> Self {
        Self {
            client_ip: extract_client_ip(request, remote_addr),
            method: request.method().to_string(),
            host: header_value(request, "host").unwrap_or_default(),
            path: request.uri().path().to_string(),
            query: request.uri().query().map(str::to_string),
            user_agent: header_value(request, "user-agent"),
            referer: header_value(request, "referer"),
        }
    }
}

fn extract_client_ip(request: &Request<Incoming>, remote_addr: SocketAddr) -> String {
    header_value(request, "x-forwarded-for")
        .and_then(|value| value.split(',').next().map(|ip| ip.trim().to_string()))
        .or_else(|| header_value(request, "x-real-ip"))
        .unwrap_or_else(|| remote_addr.ip().to_string())
}

fn header_value(request: &Request<Incoming>, name: &str) -> Option<String> {
    request
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn boxed_body(body: impl Into<Bytes>) -> ProxyBody {
    Full::new(body.into())
        .map_err(|never: Infallible| match never {})
        .boxed()
}

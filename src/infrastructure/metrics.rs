use std::convert::Infallible;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ServerBuilder;
use prometheus::{Encoder, IntCounterVec, Opts, Registry, TextEncoder};
use tokio::net::TcpListener;

#[derive(Clone)]
pub struct Metrics {
    registry: Registry,
    requests_by_path_and_ip: IntCounterVec,
    tcp_connections: IntCounterVec,
}

impl Metrics {
    pub fn new() -> Self {
        let registry = Registry::new();
        let requests_by_path_and_ip = IntCounterVec::new(
            Opts::new(
                "trafficsniffer_requests_total",
                "Total number of proxied HTTP requests by path and client IP",
            ),
            &["path", "client_ip"],
        )
        .expect("invalid Prometheus metric definition");
        registry
            .register(Box::new(requests_by_path_and_ip.clone()))
            .expect("failed to register Prometheus metric");
        let tcp_connections = IntCounterVec::new(
            Opts::new(
                "trafficsniffer_tcp_connections_total",
                "Total number of accepted TCP connections",
            ),
            &["client_ip", "listen_port", "transport"],
        )
        .expect("invalid Prometheus TCP metric definition");
        registry
            .register(Box::new(tcp_connections.clone()))
            .expect("failed to register Prometheus TCP metric");

        Self {
            registry,
            requests_by_path_and_ip,
            tcp_connections,
        }
    }

    pub fn record_request(&self, path: &str, client_ip: &str) {
        self.requests_by_path_and_ip
            .with_label_values(&[path, client_ip])
            .inc();
    }

    pub fn record_tcp_connection(
        &self,
        client_ip: &str,
        // client_port: u16,
        listen_port: u16,
        transport: &str,
    ) {
        let listen_port = listen_port.to_string();
        self.tcp_connections
            .with_label_values(&[client_ip, &listen_port, transport])
            .inc();
    }

    fn render(&self) -> Vec<u8> {
        let metric_families = self.registry.gather();
        let mut output = Vec::new();
        TextEncoder::new()
            .encode(&metric_families, &mut output)
            .expect("failed to encode Prometheus metrics");
        output
    }
}

pub async fn run_metrics_listener(listener: TcpListener, metrics: Arc<Metrics>) {
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(connection) => connection,
            Err(error) => {
                eprintln!("metrics accept error: {}", error);
                continue;
            }
        };
        let metrics = metrics.clone();

        tokio::spawn(async move {
            let service = service_fn(move |request| {
                let metrics = metrics.clone();
                async move { respond_to_metrics_request(request, metrics).await }
            });
            if let Err(error) = ServerBuilder::new(TokioExecutor::new())
                .serve_connection(TokioIo::new(stream), service)
                .await
            {
                eprintln!("metrics connection error: {}", error);
            }
        });
    }
}

async fn respond_to_metrics_request(
    request: Request<Incoming>,
    metrics: Arc<Metrics>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    if request.method() == Method::GET && request.uri().path() == "/metrics" {
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/plain; version=0.0.4")
            .body(Full::new(Bytes::from(metrics.render())))
            .unwrap());
    }

    Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Full::new(Bytes::from_static(b"Not Found")))
        .unwrap())
}

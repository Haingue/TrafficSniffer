use std::sync::Arc;

use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ServerBuilder;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use crate::application::proxy_service::ProxyService;
use crate::infrastructure::metrics::Metrics;

pub async fn run_listener(
    listener: TcpListener,
    proxy_service: Arc<ProxyService>,
    tls_acceptor: Option<TlsAcceptor>,
    metrics: Arc<Metrics>,
) {
    let listen_port = listener
        .local_addr()
        .map(|address| address.port())
        .unwrap_or_default();
    loop {
        let (stream, remote_addr) = match listener.accept().await {
            Ok(connection) => connection,
            Err(error) => {
                eprintln!("accept error: {}", error);
                continue;
            }
        };
        let service = proxy_service.clone();
        let transport = if tls_acceptor.is_some() {
            "https"
        } else {
            "http"
        };
        metrics.record_tcp_connection(
            &remote_addr.ip().to_string(),
            listen_port,
            transport,
        );

        if let Some(tls_acceptor) = tls_acceptor.clone() {
            tokio::spawn(async move {
                match tls_acceptor.accept(stream).await {
                    Ok(tls_stream) => serve_connection(tls_stream, remote_addr, service).await,
                    Err(error) => eprintln!("TLS handshake error: {}", error),
                }
            });
        } else {
            tokio::spawn(async move {
                serve_connection(stream, remote_addr, service).await;
            });
        }
    }
}

async fn serve_connection<S>(
    stream: S,
    remote_addr: std::net::SocketAddr,
    service: Arc<ProxyService>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let http_service = service_fn(move |request| {
        let service = service.clone();
        async move { service.forward_request(request, remote_addr).await }
    });

    if let Err(error) = ServerBuilder::new(TokioExecutor::new())
        .serve_connection(TokioIo::new(stream), http_service)
        .await
    {
        eprintln!("connection error: {}", error);
    }
}

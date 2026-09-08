mod application;
mod domain;
mod infrastructure;

use std::fs::OpenOptions;
use std::sync::Arc;

use hyper::Uri;
use tokio::net::TcpListener;

use application::proxy_service::ProxyService;
use domain::config::ProxyConfig;
use infrastructure::cli::CommandLineOptions;
use infrastructure::metrics::{run_metrics_listener, Metrics};
use infrastructure::server::run_listener;
use infrastructure::tls::{build_proxy_client, load_incoming_tls_acceptor};
use infrastructure::traffic_log::TrafficLogger;

#[tokio::main]
async fn main() {
    // Phase 1: initialize cryptography before any TLS component is created.
    install_crypto_provider();

    // Phase 2: read and validate the runtime options.
    let options = CommandLineOptions::parse();
    let listen_addr = parse_listen_address(&options.listen);
    let target_uri = parse_target_uri(&options.target);
    let metrics_addr = parse_listen_address(&options.metrics_listen);

    // Phase 3: build the application dependencies.
    let log_file = options.log_path.as_deref().map(open_log_file);
    let proxy_config = build_proxy_config(listen_addr, target_uri.clone());
    let logger = TrafficLogger::new(log_file, options.console);
    let client = build_proxy_client(options.insecure_target_tls);
    let metrics = Arc::new(Metrics::new());
    let proxy_service = Arc::new(ProxyService::new(
        client,
        Arc::new(proxy_config),
        logger,
        metrics.clone(),
    ));

    // Phase 4: configure the optional incoming HTTPS termination.
    let tls_acceptor = options
        .tls_cert
        .as_deref()
        .zip(options.tls_key.as_deref())
        .map(|(cert, key)| load_incoming_tls_acceptor(cert, key));

    // Phase 5: bind the listener and run the proxy.
    let listener = bind_listener(listen_addr).await;
    let metrics_listener = bind_listener(metrics_addr).await;

    print_startup_message(
        listen_addr,
        target_uri,
        options.log_path.as_deref(),
        tls_acceptor.is_some(),
        metrics_addr,
    );
    tokio::spawn(run_metrics_listener(metrics_listener, metrics.clone()));
    run_listener(listener, proxy_service, tls_acceptor, metrics).await;
}

fn install_crypto_provider() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install the Rustls ring crypto provider");
}

fn parse_listen_address(value: &str) -> std::net::SocketAddr {
    value
        .parse()
        .unwrap_or_else(|_| panic!("invalid --listen address: {}", value))
}

fn parse_target_uri(value: &str) -> Uri {
    value
        .parse()
        .unwrap_or_else(|_| panic!("invalid --target URL: {}", value))
}

fn open_log_file(path: &std::path::Path) -> std::fs::File {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap_or_else(|error| panic!("cannot open log file {}: {}", path.display(), error))
}

fn build_proxy_config(listen_addr: std::net::SocketAddr, target_uri: Uri) -> ProxyConfig {
    let target_host = target_uri.host().unwrap_or("").to_string();
    let target_port = target_uri.port_u16().map_or_else(
        || {
            if target_uri.scheme_str() == Some("https") {
                "443".to_string()
            } else {
                "80".to_string()
            }
        },
        |port| port.to_string(),
    );

    ProxyConfig {
        listen_port: listen_addr.port().to_string(),
        target_uri,
        target_host,
        target_port,
    }
}

async fn bind_listener(listen_addr: std::net::SocketAddr) -> TcpListener {
    TcpListener::bind(listen_addr)
        .await
        .unwrap_or_else(|error| panic!("cannot bind to {}: {}", listen_addr, error))
}

fn print_startup_message(
    listen_addr: std::net::SocketAddr,
    target_uri: Uri,
    log_path: Option<&std::path::Path>,
    tls_enabled: bool,
    metrics_addr: std::net::SocketAddr,
) {
    println!(
        "TrafficSniffer listening on {} ({}), forwarding to {}, metrics on {}, log: {}",
        listen_addr,
        if tls_enabled { "HTTPS" } else { "HTTP" },
        target_uri,
        metrics_addr,
        log_path
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "disabled".to_string())
    );
}

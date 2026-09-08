use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use hyper::body::Incoming;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::{connect::HttpConnector, Client};
use hyper_util::rt::TokioExecutor;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, ServerConfig, SignatureScheme};
use tokio_rustls::TlsAcceptor;

#[derive(Debug)]
struct NoCertificateVerification;

impl ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::ED25519,
        ]
    }
}

pub type ProxyClient = Client<hyper_rustls::HttpsConnector<HttpConnector>, Incoming>;

pub fn build_proxy_client(insecure_target_tls: bool) -> ProxyClient {
    let connector = if insecure_target_tls {
        let tls_config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoCertificateVerification))
            .with_no_client_auth();
        HttpsConnectorBuilder::new()
            .with_tls_config(tls_config)
            .https_or_http()
            .enable_http1()
            .build()
    } else {
        HttpsConnectorBuilder::new()
            .with_native_roots()
            .expect("cannot load system TLS root certificates")
            .https_or_http()
            .enable_http1()
            .build()
    };

    Client::builder(TokioExecutor::new()).build(connector)
}

pub fn load_incoming_tls_acceptor(cert_path: &Path, key_path: &Path) -> TlsAcceptor {
    let cert_file = std::fs::File::open(cert_path).unwrap_or_else(|error| {
        panic!(
            "cannot open TLS certificate {}: {}",
            cert_path.display(),
            error
        )
    });
    let mut cert_reader = BufReader::new(cert_file);
    let certificates = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| {
            panic!(
                "cannot read TLS certificate {}: {}",
                cert_path.display(),
                error
            )
        });
    if certificates.is_empty() {
        panic!(
            "TLS certificate file {} contains no certificate",
            cert_path.display()
        );
    }

    let key_file = std::fs::File::open(key_path).unwrap_or_else(|error| {
        panic!(
            "cannot open TLS private key {}: {}",
            key_path.display(),
            error
        )
    });
    let mut key_reader = BufReader::new(key_file);
    let private_key = rustls_pemfile::private_key(&mut key_reader)
        .unwrap_or_else(|error| {
            panic!(
                "cannot read TLS private key {}: {}",
                key_path.display(),
                error
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "TLS private key file {} contains no private key",
                key_path.display()
            )
        });

    let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certificates, private_key)
        .unwrap_or_else(|error| panic!("invalid TLS certificate/key pair: {}", error));

    TlsAcceptor::from(Arc::new(server_config))
}

use hyper::Uri;

pub struct ProxyConfig {
    pub target_uri: Uri,
    pub listen_port: String,
    pub target_host: String,
    pub target_port: String,
}

use serde::Serialize;

#[derive(Serialize)]
pub struct TrafficRecord {
    pub timestamp: String,
    pub client_ip: String,
    pub method: String,
    pub host: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    pub listen_port: String,
    pub target_host: String,
    pub target_port: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referer: Option<String>,
    pub status_code: u16,
    pub duration_ms: u128,
}

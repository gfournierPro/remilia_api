use serde::Deserialize;

/// Socket.IO initialization response
#[derive(Debug, Deserialize)]
pub struct SocketInitResponse {
    pub sid: String,
    #[allow(dead_code)]
    pub upgrades: Vec<String>,
    #[serde(rename = "pingInterval")]
    #[allow(dead_code)]
    pub ping_interval: u32,
    #[serde(rename = "pingTimeout")]
    #[allow(dead_code)]
    pub ping_timeout: u32,
    #[serde(rename = "maxPayload")]
    #[allow(dead_code)]
    pub max_payload: u64,
}

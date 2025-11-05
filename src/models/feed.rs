use serde::{Deserialize, Serialize};

/// Socket.IO initialization response
#[derive(Debug, Deserialize)]
pub struct SocketInitResponse {
    pub sid: String,
    pub upgrades: Vec<String>,
    #[serde(rename = "pingInterval")]
    pub ping_interval: u32,
    #[serde(rename = "pingTimeout")]
    pub ping_timeout: u32,
    #[serde(rename = "maxPayload")]
    pub max_payload: u64,
}

/// User representation in feed activities
#[derive(Debug, Deserialize, Serialize)]
pub struct FeedUser {
    pub username: String,
    pub avatar: String,
}

/// Feed activity types
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "lowercase")]
pub enum FeedActivity {
    #[serde(rename = "poke")]
    Poke {
        id: String,
        timestamp: String,
        from: FeedUser,
        to: FeedUser,
    },
    #[serde(rename = "friendship")]
    Friendship {
        id: String,
        timestamp: String,
        users: Vec<FeedUser>,
    },
}

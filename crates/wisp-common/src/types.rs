use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MessageSource {
    #[default]
    UserPrompted,
    Directed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConnectionStatusEvent {
    pub server_id: String,
    pub connected: bool,
    pub error: Option<String>,
    pub last_ping_at: Option<u64>,
    pub reconnect_attempts: u32,
    pub transport_kind: String,
    pub source: String,
}

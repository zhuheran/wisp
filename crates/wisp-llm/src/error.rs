#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("SSE parse error: {0}")]
    Sse(String),
    #[error("API error ({status}): {body}")]
    Api { status: u16, body: String },
    #[error("Stream was cancelled")]
    Cancelled,
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

impl From<String> for LlmError {
    fn from(s: String) -> Self {
        LlmError::Other(s)
    }
}

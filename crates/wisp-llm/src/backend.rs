use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio_util::sync::CancellationToken;
use wisp_configs::provider::Provider;

use crate::error::LlmError;

pub type ChunkCallback = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Clone)]
pub struct StreamCallbacks {
    pub on_content: ChunkCallback,
    pub on_reasoning: ChunkCallback,
}

pub struct StreamRequest {
    pub messages: Vec<Value>,
    pub model: String,
    pub provider: Provider,
    pub parameters: Option<HashMap<String, Value>>,
    pub callbacks: StreamCallbacks,
    pub cancel: CancellationToken,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StreamOutcome {
    pub text: String,
    pub reasoning: String,
}

#[async_trait::async_trait]
pub trait LlmBackend: Send + Sync {
    async fn stream(&self, req: StreamRequest) -> Result<StreamOutcome, LlmError>;
}

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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone)]
pub enum ToolChoice {
    Auto,
    None,
    Required,
    Specific(String),
}

impl Default for ToolChoice {
    fn default() -> Self {
        ToolChoice::Auto
    }
}

pub struct StreamRequest {
    pub messages: Vec<Value>,
    pub model: String,
    pub provider: Provider,
    pub parameters: Option<HashMap<String, Value>>,
    pub callbacks: StreamCallbacks,
    pub cancel: CancellationToken,
    pub tools: Vec<ToolDefinition>,
    pub tool_choice: ToolChoice,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StreamOutcome {
    pub text: String,
    pub reasoning: String,
    pub tool_call_deltas: Vec<Value>,
}

#[async_trait::async_trait]
pub trait LlmBackend: Send + Sync {
    async fn stream(&self, req: StreamRequest) -> Result<StreamOutcome, LlmError>;
}

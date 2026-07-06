pub mod backend;
pub mod backends;
pub mod error;
pub mod sse;

pub use backend::{
    resolve_parameters, ChunkCallback, LlmBackend, ReasoningConfig, ReasoningPassback,
    StreamCallbacks, StreamOutcome, StreamRequest, ToolChoice, ToolDefinition,
};
pub use error::LlmError;

use std::sync::Arc;
use wisp_configs::provider::{ApiType, Provider};

pub fn backend_for(provider: &Provider) -> Arc<dyn LlmBackend> {
    match provider.api_type {
        ApiType::OpenAi => Arc::new(backends::openai::OpenAiBackend),
        ApiType::DeepSeek => Arc::new(backends::deepseek::DeepSeekBackend),
        ApiType::OpenAiCompatible => Arc::new(backends::compat::OpenAiCompatBackend),
    }
}

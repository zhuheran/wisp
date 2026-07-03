pub mod backend;
pub mod error;
pub mod sse;
pub mod backends;

pub use backend::{LlmBackend, StreamRequest, StreamOutcome, StreamCallbacks, ChunkCallback};
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

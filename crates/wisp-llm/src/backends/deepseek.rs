use async_trait::async_trait;

use crate::backend::{LlmBackend, StreamRequest, StreamOutcome};
use crate::error::LlmError;
use super::compat::OpenAiCompatBackend;

pub struct DeepSeekBackend;

#[async_trait]
impl LlmBackend for DeepSeekBackend {
    async fn stream(&self, req: StreamRequest) -> Result<StreamOutcome, LlmError> {
        OpenAiCompatBackend.stream(req).await
    }
}

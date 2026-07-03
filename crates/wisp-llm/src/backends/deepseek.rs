use async_trait::async_trait;

use crate::backend::{LlmBackend, StreamRequest, StreamOutcome};
use crate::error::LlmError;

pub struct DeepSeekBackend;

#[async_trait]
impl LlmBackend for DeepSeekBackend {
    async fn stream(&self, _req: StreamRequest) -> Result<StreamOutcome, LlmError> {
        todo!("Task 1.4")
    }
}

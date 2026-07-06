use async_trait::async_trait;

use super::compat::{build_chat_body, stream_with_body};
use crate::backend::{
    LlmBackend, ReasoningConfig, ReasoningPassback, StreamOutcome, StreamRequest,
};
use crate::error::LlmError;

pub struct OpenAiBackend;

#[async_trait]
impl LlmBackend for OpenAiBackend {
    fn reasoning_config(&self) -> ReasoningConfig {
        ReasoningConfig { field_name: "reasoning_content", policy: ReasoningPassback::Never }
    }

    async fn stream(&self, req: StreamRequest) -> Result<StreamOutcome, LlmError> {
        let body = build_chat_body(&req);
        stream_with_body(req, body).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasoning_config_is_never() {
        let backend = OpenAiBackend;
        let config = backend.reasoning_config();
        assert_eq!(config.policy, ReasoningPassback::Never);
    }
}

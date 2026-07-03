use async_trait::async_trait;
use serde_json::json;

use crate::backend::{
    LlmBackend, ReasoningConfig, ReasoningPassback, StreamOutcome, StreamRequest,
};
use crate::error::LlmError;
use super::compat::{build_chat_body, stream_with_body};

pub struct DeepSeekBackend;

#[async_trait]
impl LlmBackend for DeepSeekBackend {
    fn reasoning_config(&self) -> ReasoningConfig {
        ReasoningConfig {
            field_name: "reasoning_content",
            policy: ReasoningPassback::ToolTurnsOnly,
        }
    }

    async fn stream(&self, req: StreamRequest) -> Result<StreamOutcome, LlmError> {
        let mut body = build_chat_body(&req);
        body["thinking"] = json!({"type": "enabled"});
        stream_with_body(req, body).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasoning_config_is_tool_turns_only() {
        let backend = DeepSeekBackend;
        let config = backend.reasoning_config();
        assert_eq!(config.field_name, "reasoning_content");
        assert_eq!(config.policy, ReasoningPassback::ToolTurnsOnly);
    }
}

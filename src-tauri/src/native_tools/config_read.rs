use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use wisp_common::{ToolContent, ToolError, ToolResult};
use wisp_configs::ConfigManager;
use wisp_software_tools::format_result::first_text;
use wisp_software_tools::NativeTool;

#[derive(Deserialize, JsonSchema)]
pub struct ConfigReadArgs {
    /// One of: "providers", "characters", "default_responder", "chore_llm", "pipeline_config", "conversation_config".
    pub key: String,
}

pub struct ConfigRead {
    config: Arc<ConfigManager>,
}

impl ConfigRead {
    pub fn new(config: Arc<ConfigManager>) -> Self {
        ConfigRead { config }
    }
}

#[async_trait]
impl NativeTool for ConfigRead {
    fn name(&self) -> &str {
        "wisp_config_read"
    }

    fn description(&self) -> &str {
        "Read application configuration values. Instruction: pass one of the supported keys listed in the key parameter schema below. Supported keys: providers, characters, default_responder, chore_llm, pipeline_config, conversation_config."
    }

    fn schema(&self) -> Value {
        let schema = schemars::schema_for!(ConfigReadArgs);
        serde_json::to_value(&schema).unwrap_or_default()
    }

    /// Config values are JSON; fence them as a ```json block so they render
    /// cleanly. Non-JSON values (e.g. a plain default_responder id) fall back
    /// to plain text.
    fn format_to_markdown(
        &self,
        _name: &str,
        _arguments: &Value,
        result: Option<&ToolResult>,
    ) -> String {
        let mut lines: Vec<String> = Vec::new();
        let result = match result {
            Some(r) => r,
            None => {
                lines.push("> No result".to_string());
                return lines.join("\n");
            },
        };
        let text = match first_text(result) {
            Some(t) => t,
            None => return lines.join("\n"),
        };
        if result.is_error {
            lines.push("> **Error**".to_string());
        } else {
            lines.push("**Result**".to_string());
        }
        lines.push(String::new());
        match serde_json::from_str::<Value>(text) {
            Ok(parsed) => {
                let pretty =
                    serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| text.to_string());
                lines.push(format!("```json\n{pretty}\n```"));
            },
            Err(_) => {
                lines.push(text.to_string());
            },
        }
        lines.join("\n")
    }

    async fn run(&self, args: Value) -> Result<ToolResult, ToolError> {
        let args: ConfigReadArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;

        let value = match args.key.as_str() {
            "providers" => serde_json::to_string_pretty(&self.config.get_providers())
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?,
            "characters" => serde_json::to_string_pretty(&self.config.get_characters())
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?,
            "default_responder" => self.config.get_default_responder().unwrap_or_default(),
            "chore_llm" => serde_json::to_string_pretty(&self.config.get_chore_llm())
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?,
            "pipeline_config" => serde_json::to_string_pretty(&self.config.get_pipeline_config())
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?,
            "conversation_config" => {
                serde_json::to_string_pretty(&self.config.get_conversation_config())
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
            },
            other => {
                return Ok(ToolResult {
                    content: vec![ToolContent::Text {
                        text: format!("Unknown key '{other}'. Supported keys: providers, characters, default_responder, chore_llm, pipeline_config, conversation_config."),
                    }],
                    is_error: true,
                });
            },
        };

        Ok(ToolResult { content: vec![ToolContent::Text { text: value }], is_error: false })
    }
}

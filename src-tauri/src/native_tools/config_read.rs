use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use wisp_common::{ToolContent, ToolError, ToolResult};
use wisp_configs::ConfigManager;
use wisp_software_tools::NativeTool;

#[derive(Deserialize, JsonSchema)]
pub struct ConfigReadArgs {
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
        "Read a configuration value. Supported keys: \"providers\", \"characters\", \"default_responder\", \"chore_llm\"."
    }

    fn schema(&self) -> Value {
        let schema = schemars::schema_for!(ConfigReadArgs);
        serde_json::to_value(&schema).unwrap_or_default()
    }

    async fn run(&self, args: Value) -> Result<ToolResult, ToolError> {
        let args: ConfigReadArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;

        let value = match args.key.as_str() {
            "providers" => serde_json::to_string_pretty(&self.config.get_providers())
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?,
            "characters" => serde_json::to_string_pretty(&self.config.get_characters())
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?,
            "default_responder" => {
                self.config.get_default_responder().unwrap_or_default()
            }
            "chore_llm" => serde_json::to_string_pretty(&self.config.get_chore_llm())
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?,
            other => {
                return Ok(ToolResult {
                    content: vec![ToolContent::Text {
                        text: format!("Unknown key '{other}'. Supported keys: providers, characters, default_responder, chore_llm."),
                    }],
                    is_error: true,
                });
            }
        };

        Ok(ToolResult {
            content: vec![ToolContent::Text { text: value }],
            is_error: false,
        })
    }
}

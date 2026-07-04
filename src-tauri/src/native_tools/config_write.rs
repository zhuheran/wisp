use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use wisp_common::{ToolContent, ToolError, ToolResult};
use wisp_configs::ConfigManager;
use wisp_software_tools::NativeTool;
use wisp_software_tools::format_result::first_text;

#[derive(Deserialize, JsonSchema)]
pub struct ConfigWriteArgs {
    /// One of: "default_responder", "chore_llm".
    pub key: String,
    /// For "default_responder": a pal ID string or null. For "chore_llm": {"provider": "...", "model": "..."} or null.
    pub value: serde_json::Value,
}

pub struct ConfigWrite {
    config: Arc<ConfigManager>,
}

impl ConfigWrite {
    pub fn new(config: Arc<ConfigManager>) -> Self {
        ConfigWrite { config }
    }
}

#[async_trait]
impl NativeTool for ConfigWrite {
    fn name(&self) -> &str {
        "wisp_config_write"
    }

    fn description(&self) -> &str {
        "Write application configuration values. Instruction: use the key parameter to select which setting to change, and provide the new value in the value parameter. See the schema below for accepted formats."
    }

    fn schema(&self) -> Value {
        let schema = schemars::schema_for!(ConfigWriteArgs);
        serde_json::to_value(&schema).unwrap_or_default()
    }

    fn requires_confirmation(&self) -> bool {
        true
    }

    /// A write tool only needs a compact one-line confirmation; the panel
    /// header already conveys the tool name and status.
    fn format_to_markdown(
        &self,
        _name: &str,
        _arguments: &Value,
        result: Option<&ToolResult>,
    ) -> String {
        match result {
            None => "> No result".to_string(),
            Some(r) => {
                let text = first_text(r).unwrap_or("");
                if r.is_error {
                    format!("✗ {text}")
                } else {
                    format!("✓ {text}")
                }
            }
        }
    }

    async fn run(&self, args: Value) -> Result<ToolResult, ToolError> {
        let args: ConfigWriteArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;

        let result_text = match args.key.as_str() {
            "default_responder" => {
                let pal_id = match &args.value {
                    Value::Null => None,
                    Value::String(s) => Some(s.clone()),
                    _ => {
                        return Ok(ToolResult {
                            content: vec![ToolContent::Text {
                                text: "default_responder expects a string or null.".to_string(),
                            }],
                            is_error: true,
                        });
                    }
                };
                self.config
                    .set_default_responder(pal_id)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                "default_responder updated.".to_string()
            }
            "chore_llm" => {
                let chore_llm = if args.value.is_null() {
                    None
                } else {
                    serde_json::from_value::<wisp_configs::ChoreLlmRef>(args.value)
                        .map_err(|e| ToolError::ExecutionFailed(format!("invalid chore_llm: {e}")))?
                        .into()
                };
                self.config
                    .set_chore_llm(chore_llm)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                "chore_llm updated.".to_string()
            }
            other => {
                return Ok(ToolResult {
                    content: vec![ToolContent::Text {
                        text: format!("Unknown key '{other}'. Supported keys: default_responder, chore_llm."),
                    }],
                    is_error: true,
                });
            }
        };

        Ok(ToolResult {
            content: vec![ToolContent::Text { text: result_text }],
            is_error: false,
        })
    }
}

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value};
use wisp_common::{ToolContent, ToolError, ToolResult};
use wisp_configs::ConfigManager;
use wisp_software_tools::format_result::first_text;
use wisp_software_tools::NativeTool;

/// Shallowly merge `patch` into `base` at the top level. Only keys present in
/// `patch` are overwritten; nested sub-objects are replaced wholesale (which
/// is correct for our flat config sections). Non-object `patch` values leave
/// `base` unchanged.
fn merge_object(base: &mut Map<String, Value>, patch: &Value) {
    let Some(patch_map) = patch.as_object() else {
        return;
    };
    for (key, value) in patch_map {
        base.insert(key.clone(), value.clone());
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct ConfigWriteArgs {
    /// One of: "default_responder", "chore_llm", "pipeline_config", "conversation_config".
    pub key: String,
    /// For "default_responder": a pal ID string or null. For "chore_llm": {"provider": "...", "model": "..."} or null.
    /// For "pipeline_config" / "conversation_config": a partial object — only the fields you want to change.
    /// Unspecified fields keep their current values (merge semantics), so callers never need to read-then-write
    /// the full section. Example: {"key":"pipeline_config","value":{"jpeg_quality":50}} updates one field safely.
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
        "Write application configuration values. Instruction: use the key parameter to select which setting to change, and provide the new value in the value parameter. Supported keys: default_responder (string|null), chore_llm ({provider,model}|null), pipeline_config (partial object — only fields you want to change; unspecified fields keep their current values), conversation_config (partial object — same merge semantics). See the value schema below for accepted field formats."
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
            },
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
                    },
                };
                self.config
                    .set_default_responder(pal_id)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                "default_responder updated.".to_string()
            },
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
            },
            "pipeline_config" => {
                if !args.value.is_object() {
                    return Ok(ToolResult {
                        content: vec![ToolContent::Text {
                            text: "pipeline_config expects a JSON object.".to_string(),
                        }],
                        is_error: true,
                    });
                }
                // Merge the patch onto the current config so unspecified fields
                // keep their existing values, then normalise & persist.
                let mut merged = serde_json::to_value(self.config.get_pipeline_config())
                    .unwrap_or_else(|_| Value::Object(Map::new()));
                if let Some(base) = merged.as_object_mut() {
                    merge_object(base, &args.value);
                }
                let pipeline: wisp_configs::PipelineConfig = serde_json::from_value(merged)
                    .map_err(|e| {
                        ToolError::ExecutionFailed(format!("invalid pipeline_config: {e}"))
                    })?;
                let pipeline = pipeline.normalize();
                self.config
                    .update_pipeline_config(pipeline)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                "pipeline_config updated.".to_string()
            },
            "conversation_config" => {
                if !args.value.is_object() {
                    return Ok(ToolResult {
                        content: vec![ToolContent::Text {
                            text: "conversation_config expects a JSON object.".to_string(),
                        }],
                        is_error: true,
                    });
                }
                let mut merged = serde_json::to_value(self.config.get_conversation_config())
                    .unwrap_or_else(|_| Value::Object(Map::new()));
                if let Some(base) = merged.as_object_mut() {
                    merge_object(base, &args.value);
                }
                let conversation: wisp_configs::ConversationLoopConfig =
                    serde_json::from_value(merged).map_err(|e| {
                        ToolError::ExecutionFailed(format!("invalid conversation_config: {e}"))
                    })?;
                let conversation = conversation.normalize();
                self.config
                    .update_conversation_config(conversation)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                "conversation_config updated.".to_string()
            },
            other => {
                return Ok(ToolResult {
                    content: vec![ToolContent::Text {
                        text: format!("Unknown key '{other}'. Supported keys: default_responder, chore_llm, pipeline_config, conversation_config."),
                    }],
                    is_error: true,
                });
            },
        };

        Ok(ToolResult {
            content: vec![ToolContent::Text { text: result_text }],
            is_error: false,
        })
    }
}

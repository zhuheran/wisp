use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use wisp_configs::model::Model;

pub type ChunkCallback = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Clone)]
pub struct StreamCallbacks {
    pub on_content: ChunkCallback,
    pub on_reasoning: ChunkCallback,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReasoningPassback {
    Never,
    Always,
    ToolTurnsOnly,
}

#[derive(Debug, Clone, Copy)]
pub struct ReasoningConfig {
    pub field_name: &'static str,
    pub policy: ReasoningPassback,
}

impl Default for ReasoningConfig {
    fn default() -> Self {
        ReasoningConfig { field_name: "reasoning_content", policy: ReasoningPassback::Never }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StreamOutcome {
    pub text: String,
    pub reasoning: String,
    /// Complete tool calls aggregated by rig from streamed deltas
    /// (`StreamingCompletionResponse.choice`). Arguments are fully parsed JSON.
    pub tool_calls: Vec<rig_core::message::ToolCall>,
    /// True when the caller cancelled the stream mid-flight. Cancellation is a
    /// normal termination, not an error: partial content is preserved and the
    /// caller decides how to persist it.
    pub cancelled: bool,
}

/// Resolve the effective request parameters by layering runtime parameters
/// on top of the model's configured defaults. Runtime values take precedence;
/// model defaults fill in any missing keys. Returns `None` if neither source
/// provides any parameters.
pub fn resolve_parameters(
    model: Option<&Model>,
    runtime: Option<&HashMap<String, Value>>,
) -> Option<HashMap<String, Value>> {
    let mut merged = model.map(|m| m.default_parameters()).unwrap_or_default();

    if let Some(rt) = runtime {
        for (k, v) in rt {
            merged.insert(k.clone(), v.clone());
        }
    }

    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wisp_configs::model::{
        Model, ModelInfo, ModelMetadata, TextGenerationParams, TextModelCapability,
    };

    fn text_model_with_params(params: TextGenerationParams) -> Model {
        Model {
            metadata: ModelMetadata {
                name: "m1".to_string(),
                display_name: "M1".to_string(),
                description: None,
                context_window: None,
                owned_by: None,
            },
            model_info: ModelInfo::TextGeneration {
                parameters: params,
                capabilities: vec![TextModelCapability::ToolUse],
                multimodal: None,
            },
        }
    }

    #[test]
    fn resolve_parameters_uses_model_defaults_when_no_runtime() {
        let model = text_model_with_params(TextGenerationParams {
            temperature: Some(0.5),
            max_tokens: Some(1024),
            ..Default::default()
        });
        let resolved = resolve_parameters(Some(&model), None).expect("should have params");

        assert_eq!(resolved.get("temperature"), Some(&serde_json::json!(0.5)));
        assert_eq!(resolved.get("max_tokens"), Some(&serde_json::json!(1024)));
        // unset Option fields must be absent
        assert!(resolved.get("top_p").is_none());
        assert!(resolved.get("seed").is_none());
    }

    #[test]
    fn resolve_parameters_runtime_overrides_model_defaults() {
        let model = text_model_with_params(TextGenerationParams {
            temperature: Some(0.5),
            max_tokens: Some(1024),
            ..Default::default()
        });
        let mut runtime = HashMap::new();
        runtime.insert("temperature".to_string(), serde_json::json!(0.9));

        let resolved =
            resolve_parameters(Some(&model), Some(&runtime)).expect("should have params");

        assert_eq!(resolved.get("temperature"), Some(&serde_json::json!(0.9)));
        assert_eq!(resolved.get("max_tokens"), Some(&serde_json::json!(1024)));
    }

    #[test]
    fn resolve_parameters_returns_none_when_both_empty() {
        let model = text_model_with_params(TextGenerationParams::default());
        assert!(resolve_parameters(Some(&model), None).is_none());
        assert!(resolve_parameters(None, None).is_none());
    }

    #[test]
    fn resolve_parameters_runtime_only_without_model_config() {
        let mut runtime = HashMap::new();
        runtime.insert("temperature".to_string(), serde_json::json!(1.2));

        let resolved = resolve_parameters(None, Some(&runtime)).expect("should have params");
        assert_eq!(resolved.get("temperature"), Some(&serde_json::json!(1.2)));
    }
}

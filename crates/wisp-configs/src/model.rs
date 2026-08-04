use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ========== COMMON STRUCTURES ==========
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TextModelCapability {
    FIM,
    ToolUse,
    Reasoning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    /// The model's primary (text) context window in tokens. This is an
    /// intrinsic property of the model, not a sampling parameter. Defaults to
    /// 128k for configs authored before this field existed.
    #[serde(
        default = "default_context_window",
        skip_serializing_if = "Option::is_none"
    )]
    pub context_window: Option<u32>,
    /// The organization or entity that owns the model (from provider model
    /// listing, e.g. `openai` / `deepseek`). Backward compatible with configs
    /// authored before this field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owned_by: Option<String>,
}

fn default_context_window() -> Option<u32> {
    Some(128_000)
}

// ========== MODEL-SPECIFIC CONFIGS ==========
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TextGenerationParams {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub max_tokens: Option<i32>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub stop_sequences: Option<Vec<String>>,
    pub seed: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImageGenerationParams {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub steps: Option<u32>,
    pub cfg_scale: Option<f32>,
    pub sampler: Option<String>,
    pub style_preset: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmbeddingParams {
    pub embedding_dim: Option<usize>,
    pub normalize: Option<bool>,
    pub truncate: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RerankerParams {
    pub top_n: Option<usize>,
    pub return_documents: Option<bool>,
    pub score_threshold: Option<f32>,
}

// ========== MULTIMODAL SUPPORT ==========
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VisionSupport {
    pub max_resolution: Option<(u32, u32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AudioSupport {
    pub sample_rate: Option<u32>,
    pub max_duration: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MultimodalConfig {
    pub vision: Option<VisionSupport>,
    pub audio: Option<AudioSupport>,
}

// ========== MODEL TYPE ENUM ==========
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "configs")]
pub enum ModelInfo {
    #[serde(rename = "text_generation")]
    TextGeneration {
        parameters: TextGenerationParams,
        capabilities: Vec<TextModelCapability>,
        multimodal: Option<MultimodalConfig>,
    },

    #[serde(rename = "image_generation")]
    ImageGeneration { parameters: ImageGenerationParams },

    #[serde(rename = "embedding")]
    Embedding { parameters: EmbeddingParams },

    #[serde(rename = "reranker")]
    Reranker { parameters: RerankerParams },

    #[serde(rename = "audio")]
    Audio {
        // Audio-specific config
    },
}

// ========== TOP-LEVEL MODEL STRUCT ==========
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub metadata: ModelMetadata,
    pub model_info: ModelInfo,
}

impl Model {
    /// Serialize the model's configured `parameters` into a flat key-value map,
    /// dropping any `null` (unset `Option`) entries. Used as fallback defaults
    /// when building LLM requests.
    pub fn default_parameters(&self) -> HashMap<String, serde_json::Value> {
        let params_value = match &self.model_info {
            ModelInfo::TextGeneration { parameters, .. } => serde_json::to_value(parameters),
            ModelInfo::ImageGeneration { parameters } => serde_json::to_value(parameters),
            ModelInfo::Embedding { parameters } => serde_json::to_value(parameters),
            ModelInfo::Reranker { parameters } => serde_json::to_value(parameters),
            ModelInfo::Audio { .. } => return HashMap::new(),
        };

        let obj = match params_value {
            Ok(serde_json::Value::Object(map)) => map,
            _ => return HashMap::new(),
        };

        obj.into_iter().filter(|(_, v)| !v.is_null()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn metadata(name: &str) -> ModelMetadata {
        ModelMetadata {
            name: name.to_string(),
            display_name: name.to_string(),
            description: None,
            context_window: None,
            owned_by: None,
        }
    }

    #[test]
    fn owned_by_serializes_when_present() {
        let md = ModelMetadata { owned_by: Some("deepseek".to_string()), ..metadata("m") };
        let value = serde_json::to_value(&md).unwrap();
        assert_eq!(value["owned_by"], "deepseek");
    }

    #[test]
    fn owned_by_is_omitted_when_absent() {
        let value = serde_json::to_value(metadata("m")).unwrap();
        assert!(value.get("owned_by").is_none());
    }

    #[test]
    fn owned_by_defaults_to_none_on_legacy_configs() {
        let md: ModelMetadata =
            serde_json::from_value(json!({ "name": "m", "display_name": "M" })).unwrap();
        assert!(md.owned_by.is_none());
    }

    #[test]
    fn owned_by_round_trips() {
        let md = ModelMetadata { owned_by: Some("openai".to_string()), ..metadata("m") };
        let value = serde_json::to_value(&md).unwrap();
        let back: ModelMetadata = serde_json::from_value(value).unwrap();
        assert_eq!(back.owned_by.as_deref(), Some("openai"));
    }
}

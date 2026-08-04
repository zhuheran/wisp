//! Provider model-management command: fetch the provider's `/models` listing
//! through the rig client (the API key never leaves the backend) and map rig
//! models onto wisp-configs models with capability inference.

use std::sync::Mutex;


use tauri::{AppHandle, Manager};
use wisp_configs::model::{
    EmbeddingParams, ImageGenerationParams, Model, ModelInfo, ModelMetadata, RerankerParams,
    TextGenerationParams, TextModelCapability,
};

use crate::types::AppData;

/// Infer the wisp `ModelInfo` (type + capabilities) from a model id (§11b).
pub fn infer_model_info(id: &str) -> ModelInfo {
    let lower = id.to_ascii_lowercase();
    if lower.contains("embed") {
        return ModelInfo::Embedding { parameters: EmbeddingParams::default() };
    }
    if lower.contains("rerank") {
        return ModelInfo::Reranker { parameters: RerankerParams::default() };
    }
    if ["dall-e", "gpt-image", "image"]
        .iter()
        .any(|key| lower.contains(key))
    {
        return ModelInfo::ImageGeneration { parameters: ImageGenerationParams::default() };
    }
    if ["tts", "whisper", "audio"].iter().any(|key| lower.contains(key)) {
        return ModelInfo::Audio {};
    }

    let mut capabilities = vec![TextModelCapability::ToolUse];
    let reasoning = lower.contains("reasoning")
        || lower.contains("reasoner")
        || lower.contains("thinking")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || lower.starts_with("o4")
        || lower.starts_with("r1")
        || lower.contains("deepseek-reasoner");
    if reasoning {
        capabilities.push(TextModelCapability::Reasoning);
    }
    if lower.contains("coder") || lower.contains("code") {
        capabilities.push(TextModelCapability::FIM);
    }
    ModelInfo::TextGeneration {
        parameters: TextGenerationParams::default(),
        capabilities,
        multimodal: None,
    }
}

/// Map a rig listing model onto a wisp-configs `Model`.
pub fn to_wisp_model(model: rig_core::model::Model) -> Model {
    let id = model.id;
    Model {
        metadata: ModelMetadata {
            name: id.clone(),
            display_name: id.clone(),
            description: model.description,
            context_window: model.context_length,
            owned_by: model.owned_by,
        },
        model_info: infer_model_info(&id),
    }
}

/// Fetch the provider's model list via the rig client and map it to wisp
/// models. Requires a stored API key (or `OPENAI_API_KEY`); the key never
/// crosses the Tauri boundary.
#[tauri::command]
pub async fn provider_fetch_models(
    app_handle: AppHandle,
    name: String,
) -> Result<Vec<Model>, String> {
    let provider = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        state
            .config_manager
            .get_provider(&name)
            .ok_or_else(|| format!("Provider '{name}' not found"))?
            .clone()
    };
    let models = wisp_llm::list_models(&provider)
        .await
        .map_err(|e| e.to_string())?;
    Ok(models.into_iter().map(to_wisp_model).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig_model(id: &str) -> rig_core::model::Model {
        rig_core::model::Model::new(id, id)
    }

    #[test]
    fn embedding_models_map_to_embedding_type() {
        assert!(matches!(
            infer_model_info("text-embedding-3-small"),
            ModelInfo::Embedding { .. }
        ));
        assert!(matches!(infer_model_info("embed-v3"), ModelInfo::Embedding { .. }));
    }

    #[test]
    fn rerank_models_map_to_reranker_type() {
        assert!(matches!(
            infer_model_info("bge-reranker-v2-m3"),
            ModelInfo::Reranker { .. }
        ));
    }

    #[test]
    fn image_models_map_to_image_generation() {
        assert!(matches!(infer_model_info("dall-e-3"), ModelInfo::ImageGeneration { .. }));
        assert!(matches!(infer_model_info("gpt-image-1"), ModelInfo::ImageGeneration { .. }));
    }

    #[test]
    fn audio_models_map_to_audio() {
        assert!(matches!(infer_model_info("whisper-1"), ModelInfo::Audio { .. }));
        assert!(matches!(infer_model_info("tts-1"), ModelInfo::Audio { .. }));
    }

    #[test]
    fn text_models_default_to_text_generation_with_tool_use() {
        let ModelInfo::TextGeneration { capabilities, .. } =
            infer_model_info("gpt-4o-mini")
        else {
            panic!("expected text_generation");
        };
        assert!(capabilities.contains(&TextModelCapability::ToolUse));
        assert!(!capabilities.contains(&TextModelCapability::Reasoning));
        assert!(!capabilities.contains(&TextModelCapability::FIM));
    }

    #[test]
    fn reasoning_markers_are_detected() {
        for id in ["deepseek-reasoner", "o1-mini", "o3", "gpt-4o-reasoning", "r1-0528", "thinking-v1"]
        {
            let ModelInfo::TextGeneration { capabilities, .. } = infer_model_info(id) else {
                panic!("{id}: expected text_generation");
            };
            assert!(
                capabilities.contains(&TextModelCapability::Reasoning),
                "{id}: expected Reasoning capability"
            );
        }
    }

    #[test]
    fn fim_markers_are_detected() {
        for id in ["deepseek-coder", "qwen2.5-coder"] {
            let ModelInfo::TextGeneration { capabilities, .. } = infer_model_info(id) else {
                panic!("{id}: expected text_generation");
            };
            assert!(capabilities.contains(&TextModelCapability::FIM), "{id}: expected FIM");
        }
    }

    #[test]
    fn to_wisp_model_maps_listing_fields() {
        let mut rig = rig_model("deepseek-chat");
        rig.description = Some("chat model".to_string());
        rig.context_length = Some(65536);
        rig.owned_by = Some("deepseek".to_string());
        let model = to_wisp_model(rig);
        assert_eq!(model.metadata.name, "deepseek-chat");
        assert_eq!(model.metadata.display_name, "deepseek-chat");
        assert_eq!(model.metadata.description.as_deref(), Some("chat model"));
        assert_eq!(model.metadata.context_window, Some(65536));
        assert_eq!(model.metadata.owned_by.as_deref(), Some("deepseek"));
        assert!(matches!(
            model.model_info,
            ModelInfo::TextGeneration { .. }
        ));
    }

    #[test]
    fn to_wisp_model_infers_reasoning_capability() {
        let model = to_wisp_model(rig_model("deepseek-reasoner"));
        let ModelInfo::TextGeneration { capabilities, .. } = model.model_info else {
            panic!("expected text_generation");
        };
        assert!(capabilities.contains(&TextModelCapability::Reasoning));
    }
}

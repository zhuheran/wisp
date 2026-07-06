pub mod compat;
pub mod deepseek;
pub mod openai;

#[cfg(test)]
mod tests {
    use super::super::*;
    use std::collections::HashMap;
    use wisp_configs::model::{
        Model, ModelInfo, ModelMetadata, TextGenerationParams, TextModelCapability,
    };
    use wisp_configs::provider::{ApiType, Provider};

    fn provider_with(api_type: ApiType) -> Provider {
        Provider {
            name: "test".to_string(),
            display_name: "Test".to_string(),
            base_url: "http://localhost".to_string(),
            models: vec![],
            api_type,
        }
    }

    fn text_model_with_params(params: TextGenerationParams) -> Model {
        Model {
            metadata: ModelMetadata {
                name: "m1".to_string(),
                display_name: "M1".to_string(),
                description: None,
                context_window: None,
            },
            model_info: ModelInfo::TextGeneration {
                parameters: params,
                capabilities: vec![TextModelCapability::ToolUse],
                multimodal: None,
            },
        }
    }

    #[test]
    fn factory_returns_compat_by_default() {
        let p = provider_with(ApiType::OpenAiCompatible);
        let _backend = backend_for(&p);
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

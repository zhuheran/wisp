use crate::model::Model;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use wisp_keyring::{KeyManager, KeyManagerError};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApiType {
    OpenAi,
    DeepSeek,
    Anthropic,
    Azure,
    Doubleword,
    Cohere,
    Gemini,
    Groq,
    HuggingFace,
    Hyperbolic,
    Llamafile,
    MiniMax,
    Mira,
    Mistral,
    Moonshot,
    Ollama,
    OpenRouter,
    Perplexity,
    Together,
    XAi,
    XiaomiMiMo,
    ZAi,
    #[default]
    OpenAiCompatible,
}

impl ApiType {
    pub const ALL: &'static [Self] = &[
        Self::OpenAi,
        Self::DeepSeek,
        Self::Anthropic,
        Self::Azure,
        Self::Doubleword,
        Self::Cohere,
        Self::Gemini,
        Self::Groq,
        Self::HuggingFace,
        Self::Hyperbolic,
        Self::Llamafile,
        Self::MiniMax,
        Self::Mira,
        Self::Mistral,
        Self::Moonshot,
        Self::Ollama,
        Self::OpenRouter,
        Self::Perplexity,
        Self::Together,
        Self::XAi,
        Self::XiaomiMiMo,
        Self::ZAi,
        Self::OpenAiCompatible,
    ];

    pub fn supports_model_listing(&self) -> bool {
        matches!(
            self,
            Self::OpenAi
                | Self::DeepSeek
                | Self::Anthropic
                | Self::Gemini
                | Self::Mistral
                | Self::Ollama
                | Self::OpenRouter
                | Self::XiaomiMiMo
                | Self::OpenAiCompatible
        )
    }

    pub fn allows_custom_base_url(&self) -> bool {
        matches!(self, Self::Azure | Self::Llamafile | Self::Ollama | Self::OpenAiCompatible)
    }

    pub fn requires_base_url(&self) -> bool {
        matches!(self, Self::Azure | Self::OpenAiCompatible)
    }

    pub fn requires_api_key(&self) -> bool {
        !matches!(self, Self::Llamafile | Self::Ollama)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub name: String,
    pub display_name: String,
    pub base_url: String,
    pub models: Vec<Model>,
    #[serde(default)]
    pub api_type: ApiType,
}

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("KeyManager error: {0}")]
    KeyManagerError(#[from] KeyManagerError),
    #[error("Model not found: {0}")]
    ModelNotFoundError(String),
    #[error("Model already exists: {0}")]
    ModelAlreadyExistError(String),
}

#[allow(unused)]
impl Provider {
    pub fn get_api_key(&self, key_manager: &KeyManager) -> Result<String, KeyManagerError> {
        key_manager.get_api_key(&self.name)
    }

    pub fn set_api_key(&self, key_manager: &KeyManager, key: &str) -> Result<(), KeyManagerError> {
        key_manager.set_api_key(&self.name, key)
    }

    pub fn delete_api_key(&self, key_manager: &KeyManager) -> Result<(), KeyManagerError> {
        key_manager.delete_api_key(&self.name)
    }

    pub fn add_model(&mut self, model: Model) -> Result<(), ProviderError> {
        if self
            .models
            .iter()
            .any(|m| m.metadata.name == model.metadata.name)
        {
            return Err(ProviderError::ModelAlreadyExistError(model.metadata.name.clone()));
        }
        self.models.push(model);
        Ok(())
    }

    pub fn get_model(&self, name: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.metadata.name == name)
    }

    pub fn update_model(&mut self, name: &str, model: Model) -> Result<(), ProviderError> {
        if let Some(index) = self.models.iter().position(|m| m.metadata.name == name) {
            self.models[index] = model;
            Ok(())
        } else {
            Err(ProviderError::ModelNotFoundError(name.to_string()))
        }
    }

    pub fn delete_model(&mut self, name: &str) -> Result<(), ProviderError> {
        if let Some(index) = self.models.iter().position(|m| m.metadata.name == name) {
            self.models.remove(index);
            Ok(())
        } else {
            Err(ProviderError::ModelNotFoundError(name.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use serde_json::Value;

    #[test]
    fn legacy_api_types_deserialize_without_migration() {
        assert_eq!(
            serde_json::from_value::<ApiType>(Value::String("open_ai".to_string())).unwrap(),
            ApiType::OpenAi
        );
        assert_eq!(
            serde_json::from_value::<ApiType>(Value::String("deep_seek".to_string())).unwrap(),
            ApiType::DeepSeek
        );
        assert_eq!(
            serde_json::from_value::<ApiType>(Value::String("open_ai_compatible".to_string())).unwrap(),
            ApiType::OpenAiCompatible
        );
    }

    #[test]
    fn provider_types_have_unique_serialized_values() {
        let values: HashSet<String> = ApiType::ALL
            .iter()
            .map(|kind| serde_json::to_value(kind).unwrap().as_str().unwrap().to_string())
            .collect();
        assert_eq!(values.len(), ApiType::ALL.len());
    }

    #[test]
    fn base_url_requirements_match_provider_kind() {
        assert!(ApiType::OpenAiCompatible.allows_custom_base_url());
        assert!(ApiType::OpenAiCompatible.requires_base_url());
        assert!(ApiType::Ollama.allows_custom_base_url());
        assert!(!ApiType::Ollama.requires_base_url());
        assert!(!ApiType::DeepSeek.allows_custom_base_url());
        assert!(!ApiType::DeepSeek.requires_base_url());
    }

    #[test]
    fn native_listing_capabilities_match_rig_support() {
        for kind in [
            ApiType::OpenAi,
            ApiType::DeepSeek,
            ApiType::Anthropic,
            ApiType::Gemini,
            ApiType::Mistral,
            ApiType::Ollama,
            ApiType::OpenRouter,
            ApiType::XiaomiMiMo,
            ApiType::OpenAiCompatible,
        ] {
            assert!(kind.supports_model_listing(), "{kind:?} should list models");
        }
        assert!(!ApiType::Groq.supports_model_listing());
        assert!(!ApiType::Together.supports_model_listing());
    }
}

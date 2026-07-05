


use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::{fs, io};
use tauri::{AppHandle, Manager};
use thiserror::Error;
use toml;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoreLlmRef {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Config {
    #[serde(default)]
    providers: Vec<crate::provider::Provider>,
    #[serde(default)]
    characters: Vec<crate::character::Character>,
    #[serde(default)]
    default_responder_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chore_llm: Option<ChoreLlmRef>,
    #[serde(default)]
    pipeline_config: Option<crate::settings::PipelineConfig>,
    #[serde(default)]
    conversation_config: Option<crate::settings::ConversationLoopConfig>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("IO Error in ConfigManager: {0}")]
    IoError(#[from] io::Error),
    #[error("Config TOML Deserialise Error: {0}")]
    TomlDeserialiseError(#[from] toml::de::Error),
    #[error("Config TOML Serialise Error: {0}")]
    TomlSerialiseError(#[from] toml::ser::Error),
    #[error("Provider Not Found Error: {0}")]
    ProviderNotFoundError(String),
    #[error("Provider Already Exists Error: {0}")]
    ProviderAlreadyExistsError(String),
    #[error("Character Not Found Error: {0}")]
    CharacterNotFoundError(String),
    #[error("Character Already Exists Error: {0}")]
    CharacterAlreadyExistsError(String),
}

pub struct ConfigManager {
    config_path: PathBuf,
    configs: Mutex<Config>,
}

impl ConfigManager {
    pub fn new<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) -> Result<Self, String> {
        let config_dir = app_handle
            .path()
            .app_data_dir()
            .expect("Failed to get config directory");

        // 确保配置目录存在
        fs::create_dir_all(&config_dir).map_err(|e| format!("Failed to create config directory: {}", e))?;

        let config_path = config_dir.join("configs.toml");
        let toml_content = fs::read_to_string(&config_path).unwrap_or_default();

        let configs = toml::from_str::<Config>(&toml_content).unwrap_or_default();

        Ok(Self {
            config_path,
            configs: Mutex::new(configs),
        })
    }

    /// Add a new provider to the config. If the
    /// provider already exists, return ProviderAlreadyExistsError
    pub fn add_provider(&self, provider: crate::provider::Provider) -> Result<(), ConfigError> {
        println!("Adding provider: {}", provider.name);
        if self.exists_provider(&provider.name) {
            println!("provider already exists");
            return Err(ConfigError::ProviderAlreadyExistsError(
                provider.name.clone(),
            ));
        }
        let mut configs = self.configs.lock().unwrap();
        configs.providers.push(provider);
        std::mem::drop(configs); // Explicitly drop the lock before saving
        println!("provider added successfully");
        self.save()?;
        println!("provider saved successfully");
        Ok(())
    }

    /// Check if a provider with the given name exists.
    pub fn exists_provider(&self, name: &str) -> bool {
        let configs = self.configs.lock().unwrap();
        configs.providers.iter().any(|p| p.name == name)
    }

    /// Get all providers.
    pub fn get_providers(&self) -> Vec<crate::provider::Provider> {
        self.configs.lock().unwrap().providers.clone()
    }

    /// Save the current config to the file.
    ///
    /// MUST UNLOCK THE MUTEX configs BEFORE CALLING THIS METHOD
    pub fn save(&self) -> Result<(), ConfigError> {
        let config_str = toml::to_string(&self.configs)?;
        fs::write(&self.config_path, config_str)?;
        Ok(())
    }

    /// Get a provider by name.
    pub fn get_provider(&self, name: &str) -> Option<crate::provider::Provider> {
        let configs = self.configs.lock().unwrap();
        configs.providers.iter().find(|p| p.name == name).cloned()
    }

    /// Update a provider with the given name.
    /// If the provider does not exist, return ProviderNotFoundError.
    pub fn update_provider(
        &self,
        name: &str,
        provider: crate::provider::Provider,
    ) -> Result<(), ConfigError> {
        let mut configs = self.configs.lock().unwrap();
        if let Some(index) = configs.providers.iter().position(|p| p.name == name) {
            configs.providers[index] = provider;
            std::mem::drop(configs);
            self.save()?;
            Ok(())
        } else {
            Err(ConfigError::ProviderNotFoundError(name.to_string()))
        }
    }

    /// Delete a provider by name.
    /// If the provider does not exist, return ProviderNotFoundError.
    pub fn delete_provider(&self, name: &str) -> Result<(), ConfigError> {
        let mut configs = self.configs.lock().unwrap();
        if let Some(index) = configs.providers.iter().position(|p| p.name == name) {
            configs.providers.remove(index);
            std::mem::drop(configs);
            self.save()?;
            Ok(())
        } else {
            Err(ConfigError::ProviderNotFoundError(name.to_string()))
        }
    }

    // ========== Character Management ==========

    /// Get all characters
    pub fn get_characters(&self) -> Vec<crate::character::Character> {
        self.configs.lock().unwrap().characters.clone()
    }

    /// Get a character by ID
    pub fn get_character(&self, id: &str) -> Option<crate::character::Character> {
        let configs = self.configs.lock().unwrap();
        configs.characters.iter().find(|c| c.id == id).cloned()
    }

    /// Check if a character with the given ID exists
    pub fn exists_character(&self, id: &str) -> bool {
        let configs = self.configs.lock().unwrap();
        configs.characters.iter().any(|c| c.id == id)
    }

    /// Add a new character
    pub fn add_character(&self, character: crate::character::Character) -> Result<(), ConfigError> {
        if self.exists_character(&character.id) {
            return Err(ConfigError::CharacterAlreadyExistsError(
                character.id.clone(),
            ));
        }
        let mut configs = self.configs.lock().unwrap();
        configs.characters.push(character);
        std::mem::drop(configs);
        self.save()?;
        Ok(())
    }

    /// Update a character
    pub fn update_character(
        &self,
        id: &str,
        character: crate::character::Character,
    ) -> Result<(), ConfigError> {
        let mut configs = self.configs.lock().unwrap();
        if let Some(index) = configs.characters.iter().position(|c| c.id == id) {
            configs.characters[index] = character;
            std::mem::drop(configs);
            self.save()?;
            Ok(())
        } else {
            Err(ConfigError::CharacterNotFoundError(id.to_string()))
        }
    }

    /// Delete a character by ID
    pub fn delete_character(&self, id: &str) -> Result<(), ConfigError> {
        let mut configs = self.configs.lock().unwrap();
        if let Some(index) = configs.characters.iter().position(|c| c.id == id) {
            configs.characters.remove(index);
            std::mem::drop(configs);
            self.save()?;
            Ok(())
        } else {
            Err(ConfigError::CharacterNotFoundError(id.to_string()))
        }
    }

    // ========== Default Responder ==========

    /// Get the default responder ID
    pub fn get_default_responder(&self) -> Option<String> {
        self.configs.lock().unwrap().default_responder_id.clone()
    }

    /// Set the default responder ID
    pub fn set_default_responder(&self, character_id: Option<String>) -> Result<(), ConfigError> {
        let mut configs = self.configs.lock().unwrap();
        configs.default_responder_id = character_id;
        std::mem::drop(configs);
        self.save()?;
        Ok(())
    }

    // ========== Chore LLM ==========

    /// Get the chore LLM reference.
    pub fn get_chore_llm(&self) -> Option<ChoreLlmRef> {
        self.configs.lock().unwrap().chore_llm.clone()
    }

    /// Set the chore LLM reference.
    pub fn set_chore_llm(&self, chore_llm: Option<ChoreLlmRef>) -> Result<(), ConfigError> {
        let mut configs = self.configs.lock().unwrap();
        configs.chore_llm = chore_llm;
        std::mem::drop(configs);
        self.save()?;
        Ok(())
    }

    // ========== Pipeline Config ==========

    /// Get the pipeline config, returning the default if not set.
    pub fn get_pipeline_config(&self) -> crate::settings::PipelineConfig {
        self.configs
            .lock()
            .unwrap()
            .pipeline_config
            .clone()
            .unwrap_or_default()
    }

    /// Update the pipeline config.
    pub fn update_pipeline_config(
        &self,
        config: crate::settings::PipelineConfig,
    ) -> Result<(), ConfigError> {
        let mut configs = self.configs.lock().unwrap();
        configs.pipeline_config = Some(config);
        std::mem::drop(configs);
        self.save()?;
        Ok(())
    }

    // ========== Conversation Config ==========

    /// Get the conversation loop config, returning the default if not set.
    pub fn get_conversation_config(&self) -> crate::settings::ConversationLoopConfig {
        self.configs
            .lock()
            .unwrap()
            .conversation_config
            .clone()
            .unwrap_or_default()
    }

    /// Update the conversation loop config.
    pub fn update_conversation_config(
        &self,
        config: crate::settings::ConversationLoopConfig,
    ) -> Result<(), ConfigError> {
        let mut configs = self.configs.lock().unwrap();
        configs.conversation_config = Some(config);
        std::mem::drop(configs);
        self.save()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_with_settings_serializes_to_toml() {
        let config = Config {
            providers: vec![],
            characters: vec![],
            default_responder_id: None,
            chore_llm: None,
            pipeline_config: Some(crate::settings::PipelineConfig {
                jpeg_quality: 50,
                ..Default::default()
            }),
            conversation_config: Some(crate::settings::ConversationLoopConfig {
                max_tool_rounds: 7,
                ..Default::default()
            }),
        };

        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("jpeg_quality = 50"));
        assert!(toml_str.contains("max_tool_rounds = 7"));
    }

    #[test]
    fn config_without_settings_deserializes_from_empty_toml() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.pipeline_config.is_none());
        assert!(config.conversation_config.is_none());
    }

    #[test]
    fn config_with_settings_roundtrips() {
        let config = Config {
            providers: vec![],
            characters: vec![],
            default_responder_id: None,
            chore_llm: None,
            pipeline_config: Some(crate::settings::PipelineConfig::default()),
            conversation_config: Some(crate::settings::ConversationLoopConfig {
                retry_attempts: 5,
                ..Default::default()
            }),
        };

        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            deserialized.conversation_config.as_ref().unwrap().retry_attempts,
            5
        );
    }
}

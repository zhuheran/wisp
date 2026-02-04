use super::model::Model;
use crate::key_manager::{KeyManager, KeyManagerError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub name: String,
    pub display_name: String,
    pub base_url: String,
    pub models: Vec<Model>,
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
            return Err(ProviderError::ModelAlreadyExistError(
                model.metadata.name.clone(),
            ));
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

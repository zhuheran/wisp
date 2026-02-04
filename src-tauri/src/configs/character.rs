use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterParameter {
    pub name: String,
    pub value: serde_json::Value,
    pub metadata: Option<ParameterMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterMetadata {
    pub label: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub id: String,
    pub name: String,
    pub alias: Option<String>,
    pub avatar: Option<String>,
    pub description: String,
    pub system_prompt: String,
    pub parameters: Vec<CharacterParameter>,
    pub model_id: String,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Character {
    pub fn new(
        id: String,
        name: String,
        description: String,
        system_prompt: String,
        model_id: String,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            id,
            name,
            alias: None,
            avatar: None,
            description,
            system_prompt,
            parameters: Vec::new(),
            model_id,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn get_parameter(&self, name: &str) -> Option<&CharacterParameter> {
        self.parameters.iter().find(|p| p.name == name)
    }

    pub fn set_parameter(
        &mut self,
        name: String,
        value: serde_json::Value,
        metadata: Option<ParameterMetadata>,
    ) {
        if let Some(index) = self.parameters.iter().position(|p| p.name == name) {
            self.parameters[index].value = value;
            self.parameters[index].metadata = metadata;
        } else {
            self.parameters.push(CharacterParameter {
                name,
                value,
                metadata,
            });
        }
        self.touch();
    }

    pub fn remove_parameter(&mut self, name: &str) {
        self.parameters.retain(|p| p.name != name);
        self.touch();
    }

    pub fn touch(&mut self) {
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }

    /// Convert parameters to a HashMap for easier consumption
    pub fn parameters_to_map(&self) -> HashMap<String, serde_json::Value> {
        self.parameters
            .iter()
            .map(|p| (p.name.clone(), p.value.clone()))
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CharacterList {
    pub characters: Vec<Character>,
}

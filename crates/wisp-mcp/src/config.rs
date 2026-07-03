use crate::types::*;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};


// ========== MCP Config Manager ==========

pub struct McpConfigManager {
    config_path: PathBuf,
    config: Mutex<McpConfig>,
}

impl McpConfigManager {
    pub fn new<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) -> Result<Self, String> {
        let config_dir = app_handle
            .path()
            .app_data_dir()
            .expect("Failed to get config directory");

        fs::create_dir_all(&config_dir).map_err(|e| format!("Failed to create config directory: {}", e))?;

        let config_path = config_dir.join("mcp_config.json");
        let config = if config_path.exists() {
            let content = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            McpConfig::default()
        };

        Ok(Self {
            config_path,
            config: Mutex::new(config),
        })
    }

    pub fn save(&self) -> Result<(), String> {
        let config = self.config.lock().map_err(|e| e.to_string())?;
        let content = serde_json::to_string_pretty(&*config).map_err(|e| e.to_string())?;
        fs::write(&self.config_path, content).map_err(|e| e.to_string())
    }

    pub fn get_config(&self) -> McpConfig {
        self.config.lock().unwrap().clone()
    }

    pub fn update_config(&self, config: McpConfig) -> Result<(), String> {
        let mut current = self.config.lock().map_err(|e| e.to_string())?;
        *current = config;
        drop(current);
        self.save()
    }

    // Server management
    pub fn add_server(&self, server: ServerConfig) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
        if config.servers.iter().any(|s| s.id == server.id) {
            return Err(format!("Server {} already exists", server.id));
        }
        config.servers.push(server);
        drop(config);
        self.save()
    }

    pub fn remove_server(&self, server_id: &str) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
        config.servers.retain(|s| s.id != server_id);
        drop(config);
        self.save()
    }

    pub fn update_server(&self, server_id: &str, server: ServerConfig) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
        if let Some(index) = config.servers.iter().position(|s| s.id == server_id) {
            config.servers[index] = server;
            drop(config);
            self.save()
        } else {
            Err(format!("Server {} not found", server_id))
        }
    }

    pub fn get_server(&self, server_id: &str) -> Option<ServerConfig> {
        let config = self.config.lock().unwrap();
        config.servers.iter().find(|s| s.id == server_id).cloned()
    }

    pub fn get_all_servers(&self) -> Vec<ServerConfig> {
        let config = self.config.lock().unwrap();
        config.servers.clone()
    }

    // Pipeline config
    pub fn get_pipeline_config(&self) -> PipelineConfig {
        let config = self.config.lock().unwrap();
        config.pipeline_config.clone().unwrap_or_default()
    }

    pub fn update_pipeline_config(&self, pipeline_config: PipelineConfig) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
        config.pipeline_config = Some(pipeline_config);
        drop(config);
        self.save()
    }

    // Conversation config
    pub fn get_conversation_config(&self) -> ConversationLoopConfig {
        let config = self.config.lock().unwrap();
        config.conversation_config.clone().unwrap_or_default()
    }

    pub fn update_conversation_config(&self, conversation_config: ConversationLoopConfig) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
        config.conversation_config = Some(conversation_config);
        drop(config);
        self.save()
    }
}

use super::types::*;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use crate::types::AppData;

// ========== MCP Config Manager ==========

pub struct McpConfigManager {
    config_path: PathBuf,
    config: Mutex<McpConfig>,
}

impl McpConfigManager {
    pub fn new(app_handle: &AppHandle) -> Result<Self, String> {
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

// ========== Tauri Commands ==========

// Server config commands
#[tauri::command]
pub async fn mcp_get_servers(app_handle: AppHandle) -> Result<Vec<ServerConfig>, String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(state.mcp_config_manager.get_all_servers())
}

#[tauri::command]
pub async fn mcp_get_server(app_handle: AppHandle, server_id: String) -> Result<Option<ServerConfig>, String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(state.mcp_config_manager.get_server(&server_id))
}

#[tauri::command]
pub async fn mcp_add_server(app_handle: AppHandle, server: ServerConfig) -> Result<(), String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    state.mcp_config_manager.add_server(server)
}

#[tauri::command]
pub async fn mcp_update_server(app_handle: AppHandle, server_id: String, server: ServerConfig) -> Result<(), String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    state.mcp_config_manager.update_server(&server_id, server)
}

#[tauri::command]
pub async fn mcp_remove_server(app_handle: AppHandle, server_id: String) -> Result<(), String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    state.mcp_config_manager.remove_server(&server_id)
}

// Pipeline config commands
#[tauri::command]
pub async fn mcp_get_pipeline_config(app_handle: AppHandle) -> Result<PipelineConfig, String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(state.mcp_config_manager.get_pipeline_config())
}

#[tauri::command]
pub async fn mcp_update_pipeline_config(app_handle: AppHandle, config: PipelineConfig) -> Result<(), String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    state.mcp_config_manager.update_pipeline_config(config)
}

// Conversation config commands
#[tauri::command]
pub async fn mcp_get_conversation_config(app_handle: AppHandle) -> Result<ConversationLoopConfig, String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(state.mcp_config_manager.get_conversation_config())
}

#[tauri::command]
pub async fn mcp_update_conversation_config(app_handle: AppHandle, config: ConversationLoopConfig) -> Result<(), String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    state.mcp_config_manager.update_conversation_config(config)
}

// Session persistence commands
#[tauri::command]
pub async fn mcp_save_session(app_handle: AppHandle, session: SessionState) -> Result<(), String> {
    let config_dir = app_handle
        .path()
        .app_data_dir()
        .expect("Failed to get config directory");
    
    let sessions_dir = config_dir.join("mcp_sessions");
    fs::create_dir_all(&sessions_dir).map_err(|e| e.to_string())?;
    
    let session_path = sessions_dir.join(format!("{}.json", session.id));
    let content = serde_json::to_string_pretty(&session).map_err(|e| e.to_string())?;
    fs::write(session_path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mcp_load_session(app_handle: AppHandle, session_id: String) -> Result<Option<SessionState>, String> {
    let config_dir = app_handle
        .path()
        .app_data_dir()
        .expect("Failed to get config directory");
    
    let session_path = config_dir.join("mcp_sessions").join(format!("{}.json", session_id));
    
    if !session_path.exists() {
        return Ok(None);
    }
    
    let content = fs::read_to_string(session_path).map_err(|e| e.to_string())?;
    let session = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    Ok(Some(session))
}

#[tauri::command]
pub async fn mcp_delete_session(app_handle: AppHandle, session_id: String) -> Result<(), String> {
    let config_dir = app_handle
        .path()
        .app_data_dir()
        .expect("Failed to get config directory");
    
    let session_path = config_dir.join("mcp_sessions").join(format!("{}.json", session_id));
    
    if session_path.exists() {
        fs::remove_file(session_path).map_err(|e| e.to_string())?;
    }
    
    Ok(())
}

#[tauri::command]
pub async fn mcp_list_sessions(app_handle: AppHandle) -> Result<Vec<SessionState>, String> {
    let config_dir = app_handle
        .path()
        .app_data_dir()
        .expect("Failed to get config directory");
    
    let sessions_dir = config_dir.join("mcp_sessions");
    
    if !sessions_dir.exists() {
        return Ok(vec![]);
    }
    
    let mut sessions = vec![];
    
    for entry in fs::read_dir(sessions_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            if let Ok(session) = serde_json::from_str::<SessionState>(&content) {
                sessions.push(session);
            }
        }
    }
    
    Ok(sessions)
}

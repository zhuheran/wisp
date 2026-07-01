use super::types::*;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
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
        .map_err(|e| e.to_string())?;

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
        .map_err(|e| e.to_string())?;

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
        .map_err(|e| e.to_string())?;

    let session_path = config_dir.join("mcp_sessions").join(format!("{}.json", session_id));

    if session_path.exists() {
        fs::remove_file(session_path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UiToolEntry {
    pub qualified_name: String,
    pub model_name: String,
    pub server_id: String,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
}

fn sanitize_tool_name_part(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch.to_ascii_lowercase() } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn encode_tool_name_for_model(tool_name: &str, duplicate_index: Option<usize>) -> String {
    let base = format!("mcp__{}", sanitize_tool_name_part(tool_name));
    match duplicate_index {
        Some(index) => format!("{}__{}", base, index),
        None => base,
    }
}

fn rebuild_global_tool_state(state: &mut AppData, tools: Vec<NormalizedTool>) {
    let mut model_name_map = HashMap::new();
    let mut enabled_tools = state.global_mcp_tool_state.enabled_tools.clone();
    enabled_tools.retain(|qualified_name| tools.iter().any(|tool| &tool.qualified_name == qualified_name));

    let mut grouped: HashMap<String, Vec<&NormalizedTool>> = HashMap::new();
    for tool in &tools {
        grouped.entry(tool.name.clone()).or_default().push(tool);
    }

    for group in grouped.values() {
        for (index, tool) in group.iter().enumerate() {
            let model_name = encode_tool_name_for_model(
                &tool.name,
                if group.len() > 1 { Some(index + 1) } else { None },
            );
            model_name_map.insert(model_name, tool.qualified_name.clone());
        }
    }

    state.global_mcp_tool_state.available_tools = tools;
    state.global_mcp_tool_state.enabled_tools = enabled_tools;
    state.global_mcp_tool_state.model_name_map = model_name_map;
}

#[tauri::command]
pub async fn mcp_refresh_global_tool_state(app_handle: AppHandle) -> Result<(), String> {
    let (servers, stdio_manager, http_manager) = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        (
            state.mcp_config_manager.get_all_servers(),
            std::sync::Arc::clone(&state.mcp_stdio_manager),
            std::sync::Arc::clone(&state.mcp_http_manager),
        )
    };

    let stdio_statuses = stdio_manager.get_all_statuses().await;
    let http_statuses = http_manager.get_all_statuses().await;
    let mut connected = HashSet::new();
    for status in stdio_statuses.into_iter().chain(http_statuses.into_iter()) {
        if status.connected {
            connected.insert(status.server_id);
        }
    }

    let mut normalized_tools = Vec::new();
    for server in servers.into_iter().filter(|server| connected.contains(&server.id)) {
        let raw = match server.transport {
            TransportConfig::Stdio { .. } => stdio_manager
                .list_tools(&server.id, None)
                .await
                .map_err(|e| e.to_string())?,
            TransportConfig::Sse { .. } | TransportConfig::Http { .. } => http_manager
                .list_tools(&server.id, None)
                .await
                .map_err(|e| e.to_string())?,
        };

        let tools = raw
            .get("tools")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();

        for tool in tools {
            let name = tool.get("name").and_then(|value| value.as_str()).ok_or_else(|| "Tool missing name".to_string())?;
            normalized_tools.push(NormalizedTool {
                name: name.to_string(),
                server_id: server.id.clone(),
                qualified_name: format!("{}:{}", server.id, name),
                description: tool.get("description").and_then(|value| value.as_str()).map(ToString::to_string),
                input_schema: tool.get("inputSchema").cloned().unwrap_or_else(|| serde_json::json!({ "type": "object", "properties": {} })),
                annotations: tool.get("annotations").cloned().map(serde_json::from_value).transpose().map_err(|e| e.to_string())?,
            });
        }
    }

    let state = app_handle.state::<Mutex<AppData>>();
    let mut state = state.lock().map_err(|e| e.to_string())?;
    rebuild_global_tool_state(&mut state, normalized_tools);
    Ok(())
}

#[tauri::command]
pub async fn mcp_list_global_tools(app_handle: AppHandle) -> Result<Vec<UiToolEntry>, String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;

    let mut entries = state
        .global_mcp_tool_state
        .available_tools
        .iter()
        .map(|tool| {
            let model_name = state
                .global_mcp_tool_state
                .model_name_map
                .iter()
                .find(|(_, qualified_name)| *qualified_name == &tool.qualified_name)
                .map(|(model_name, _)| model_name.clone())
                .unwrap_or_else(|| tool.name.clone());
            UiToolEntry {
                qualified_name: tool.qualified_name.clone(),
                model_name,
                server_id: tool.server_id.clone(),
                name: tool.name.clone(),
                description: tool.description.clone(),
                enabled: state.global_mcp_tool_state.enabled_tools.contains(&tool.qualified_name),
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));
    Ok(entries)
}

#[tauri::command]
pub async fn mcp_set_global_enabled_tools(app_handle: AppHandle, qualified_names: Vec<String>) -> Result<(), String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let mut state = state.lock().map_err(|e| e.to_string())?;
    let available = state
        .global_mcp_tool_state
        .available_tools
        .iter()
        .map(|tool| tool.qualified_name.clone())
        .collect::<HashSet<_>>();
    state.global_mcp_tool_state.enabled_tools = qualified_names
        .into_iter()
        .filter(|qualified_name| available.contains(qualified_name))
        .collect();
    Ok(())
}

#[tauri::command]
pub async fn mcp_set_server_enabled(app_handle: AppHandle, server_id: String, enabled: bool) -> Result<(), String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let mut state = state.lock().map_err(|e| e.to_string())?;
    let matching = state
        .global_mcp_tool_state
        .available_tools
        .iter()
        .filter(|tool| tool.server_id == server_id)
        .map(|tool| tool.qualified_name.clone())
        .collect::<Vec<_>>();
    if enabled {
        for qualified_name in matching {
            state.global_mcp_tool_state.enabled_tools.insert(qualified_name);
        }
    } else {
        for qualified_name in matching {
            state.global_mcp_tool_state.enabled_tools.remove(&qualified_name);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn mcp_list_sessions(app_handle: AppHandle) -> Result<Vec<SessionState>, String> {
    let config_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    
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

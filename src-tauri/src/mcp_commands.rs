use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

use wisp_mcp::{
    ConversationLoopConfig, PipelineConfig, ServerConfig, SessionState,
};
use crate::types::AppData;

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

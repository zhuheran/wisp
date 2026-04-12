use serde_json::Value;
use tauri::{AppHandle, Manager};
use std::sync::Mutex;

use super::manager::McpHttpManager;
use super::super::mcp::types::{ServerConfig, ConnectionStatus};
use super::super::types::AppData;

#[tauri::command]
pub async fn mcp_http_connect(
    app_handle: AppHandle,
    config: ServerConfig,
) -> Result<(), String> {
    let manager = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        std::sync::Arc::clone(&state.mcp_http_manager)
    };
    
    manager.connect_server(&config).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mcp_http_disconnect(
    app_handle: AppHandle,
    server_id: String,
) -> Result<(), String> {
    let manager = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        std::sync::Arc::clone(&state.mcp_http_manager)
    };
    
    manager.disconnect_server(&server_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mcp_http_get_status(
    app_handle: AppHandle,
    server_id: String,
) -> Result<Option<ConnectionStatus>, String> {
    let manager = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        std::sync::Arc::clone(&state.mcp_http_manager)
    };
    
    Ok(manager.get_status(&server_id).await)
}

#[tauri::command]
pub async fn mcp_http_get_all_statuses(
    app_handle: AppHandle,
) -> Result<Vec<ConnectionStatus>, String> {
    let manager = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        std::sync::Arc::clone(&state.mcp_http_manager)
    };
    
    Ok(manager.get_all_statuses().await)
}

#[tauri::command]
pub async fn mcp_http_list_tools(
    app_handle: AppHandle,
    server_id: String,
    cursor: Option<String>,
) -> Result<Value, String> {
    let manager = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        std::sync::Arc::clone(&state.mcp_http_manager)
    };
    
    manager.list_tools(&server_id, cursor).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mcp_http_call_tool(
    app_handle: AppHandle,
    server_id: String,
    tool_name: String,
    arguments: Option<Value>,
) -> Result<Value, String> {
    let manager = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        std::sync::Arc::clone(&state.mcp_http_manager)
    };
    
    manager.call_tool(&server_id, &tool_name, arguments).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mcp_http_is_connected(
    app_handle: AppHandle,
    server_id: String,
) -> Result<bool, String> {
    let manager = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        std::sync::Arc::clone(&state.mcp_http_manager)
    };
    
    Ok(manager.is_connected(&server_id).await)
}

use serde_json::Value;
use tauri::{AppHandle, Manager};
use std::sync::Mutex;

use super::manager::McpStdioManager;
use super::super::mcp::types::{ServerConfig, ConnectionStatus};
use super::super::types::AppData;

/// 连接 MCP stdio 服务器
#[tauri::command]
pub async fn mcp_stdio_connect(
    app_handle: AppHandle,
    config: ServerConfig,
) -> Result<(), String> {
    // Clone the Arc to avoid holding the lock across await
    let manager = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        std::sync::Arc::clone(&state.mcp_stdio_manager)
    };
    
    manager.connect_server(&config).await.map_err(|e| e.to_string())
}

/// 断开 MCP stdio 服务器
#[tauri::command]
pub async fn mcp_stdio_disconnect(
    app_handle: AppHandle,
    server_id: String,
) -> Result<(), String> {
    let manager = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        std::sync::Arc::clone(&state.mcp_stdio_manager)
    };
    
    manager.disconnect_server(&server_id).await.map_err(|e| e.to_string())
}

/// 获取 MCP stdio 服务器状态
#[tauri::command]
pub async fn mcp_stdio_get_status(
    app_handle: AppHandle,
    server_id: String,
) -> Result<Option<ConnectionStatus>, String> {
    let manager = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        std::sync::Arc::clone(&state.mcp_stdio_manager)
    };
    
    Ok(manager.get_status(&server_id).await)
}

/// 获取所有 MCP stdio 服务器状态
#[tauri::command]
pub async fn mcp_stdio_get_all_statuses(
    app_handle: AppHandle,
) -> Result<Vec<ConnectionStatus>, String> {
    let manager = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        std::sync::Arc::clone(&state.mcp_stdio_manager)
    };
    
    Ok(manager.get_all_statuses().await)
}

/// 列出 MCP 工具
#[tauri::command]
pub async fn mcp_stdio_list_tools(
    app_handle: AppHandle,
    server_id: String,
    cursor: Option<String>,
) -> Result<Value, String> {
    let manager = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        std::sync::Arc::clone(&state.mcp_stdio_manager)
    };
    
    manager.list_tools(&server_id, cursor).await.map_err(|e| e.to_string())
}

/// 调用 MCP 工具
#[tauri::command]
pub async fn mcp_stdio_call_tool(
    app_handle: AppHandle,
    server_id: String,
    tool_name: String,
    arguments: Option<Value>,
) -> Result<Value, String> {
    let manager = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        std::sync::Arc::clone(&state.mcp_stdio_manager)
    };
    
    manager.call_tool(&server_id, &tool_name, arguments).await.map_err(|e| e.to_string())
}

/// 检查服务器是否已连接
#[tauri::command]
pub async fn mcp_stdio_is_connected(
    app_handle: AppHandle,
    server_id: String,
) -> Result<bool, String> {
    let manager = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        std::sync::Arc::clone(&state.mcp_stdio_manager)
    };
    
    Ok(manager.is_connected(&server_id).await)
}

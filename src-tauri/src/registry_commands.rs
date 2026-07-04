use std::sync::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Manager};
use crate::types::AppData;
use wisp_mcp::{NormalizedTool, TransportConfig, register_mcp_tools};
use wisp_tool_registry::ToolDefinition;
use wisp_common::ToolResult;

/// A tool definition paired with its runtime `enabled` flag.
///
/// The registry stores enabled state separately from `ToolDefinition`
/// (which is purely descriptive), but the frontend needs the flag to render
/// the per-server toggle in the chat UI. Without it, `tool.enabled` arrives
/// as `undefined` and the toggle always reads "disabled" — making the UI
/// control a no-op.
#[derive(Serialize)]
pub struct ListedTool {
    #[serde(flatten)]
    pub definition: ToolDefinition,
    pub enabled: bool,
}

/// Refresh the registry by fetching tools from all connected MCP servers.
#[tauri::command]
pub async fn registry_refresh(app_handle: AppHandle) -> Result<(), String> {
    let (stdio_manager, http_manager, mcp_config_manager) = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        (
            std::sync::Arc::clone(&state.mcp_stdio_manager),
            std::sync::Arc::clone(&state.mcp_http_manager),
            state.mcp_config_manager.get_all_servers(),
        )
    };

    // Collect tools from all connected servers
    let stdio_statuses = stdio_manager.get_all_statuses().await;
    let http_statuses = http_manager.get_all_statuses().await;

    let mut server_tools: Vec<(String, Vec<NormalizedTool>, TransportConfig)> = Vec::new();

    for server in &mcp_config_manager {
        let is_connected = stdio_statuses
            .iter()
            .chain(http_statuses.iter())
            .any(|s| s.server_id == server.id && s.connected);

        if !is_connected {
            continue;
        }

        let transport = server.transport.clone();
        let raw = match &transport {
            TransportConfig::Stdio { .. } => stdio_manager
                .list_tools(&server.id, None)
                .await
                .map_err(|e| e.to_string())?,
            TransportConfig::Sse { .. }
            | TransportConfig::Http { .. } => http_manager
                .list_tools(&server.id, None)
                .await
                .map_err(|e| e.to_string())?,
        };

        let tools: Vec<NormalizedTool> = raw
            .get("tools")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        let name = t.get("name")?.as_str()?;
                        Some(NormalizedTool {
                            name: name.to_string(),
                            server_id: server.id.clone(),
                            qualified_name: format!("{}:{}", server.id, name),
                            description: t
                                .get("description")
                                .and_then(|v| v.as_str())
                                .map(String::from),
                            input_schema: t
                                .get("inputSchema")
                                .cloned()
                                .unwrap_or(serde_json::json!({"type":"object","properties":{}})),
                            annotations: t
                                .get("annotations")
                                .and_then(|v| serde_json::from_value(v.clone()).ok()),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        server_tools.push((server.id.clone(), tools, transport));
    }

    // Register all discovered tools in the registry
    {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        let stdio_mgr = std::sync::Arc::clone(&state.mcp_stdio_manager);
        let http_mgr = std::sync::Arc::clone(&state.mcp_http_manager);
        for (server_id, tools, transport) in &server_tools {
            register_mcp_tools(
                &state.tool_registry,
                server_id,
                tools,
                transport,
                std::sync::Arc::clone(&stdio_mgr),
                std::sync::Arc::clone(&http_mgr),
            );
        }
    }

    Ok(())
}

/// List all registered tool definitions (for frontend display).
#[tauri::command]
pub async fn registry_list_tools(app_handle: AppHandle) -> Result<Vec<ListedTool>, String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    let enabled_set = state.tool_registry.enabled_set();
    let mut tools: Vec<ListedTool> = state
        .tool_registry
        .list_tools()
        .into_iter()
        .map(|definition| {
            let enabled = enabled_set.contains(&definition.name);
            ListedTool { definition, enabled }
        })
        .collect();
    tools.sort_by(|a, b| a.definition.name.cmp(&b.definition.name));
    Ok(tools)
}

/// Execute a tool by its registered name.
#[tauri::command]
pub async fn registry_execute(
    app_handle: AppHandle,
    name: String,
    arguments: Option<serde_json::Value>,
) -> Result<ToolResult, String> {
    let args = arguments.unwrap_or(serde_json::Value::Null);
    let registry = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        std::sync::Arc::clone(&state.tool_registry)
    };
    registry
        .execute(&name, args, None)
        .await
        .map_err(|e| e.to_string())
}

/// Set which tools are enabled by their registered names.
#[tauri::command]
pub async fn registry_set_enabled(app_handle: AppHandle, names: Vec<String>) -> Result<(), String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    let names_set: std::collections::HashSet<String> = names.into_iter().collect();
    state.tool_registry.set_enabled(names_set);
    Ok(())
}

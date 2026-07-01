use crate::db;
use db::chat::Chat;
use super::cache::DiagramCache;
use super::key_manager::KeyManager;
use super::configs::ConfigManager;
use super::mcp::commands::McpConfigManager;
use super::mcp_stdio::McpStdioManager;
use super::mcp_http::McpHttpManager;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

use crate::tool_registry::ToolRegistry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConnectionStatusEvent {
    pub server_id: String,
    pub connected: bool,
    pub error: Option<String>,
    pub last_ping_at: Option<u64>,
    pub reconnect_attempts: u32,
    pub transport_kind: String,
    pub source: String,
}

pub struct AppData {
    pub chat: Chat,
    pub diagram_cache: DiagramCache,
    pub key_manager: KeyManager,
    pub config_manager: ConfigManager,
    pub mcp_config_manager: McpConfigManager,
    pub mcp_stdio_manager: Arc<McpStdioManager>,
    pub mcp_http_manager: Arc<McpHttpManager>,
    pub tool_registry: Arc<ToolRegistry>,
}

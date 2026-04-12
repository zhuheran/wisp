use crate::db;
use db::chat::Chat;
use super::cache::DiagramCache;
use super::key_manager::KeyManager;
use super::configs::ConfigManager;
use super::mcp::commands::McpConfigManager;
use super::mcp_stdio::McpStdioManager;
use super::mcp_http::McpHttpManager;
use std::sync::Arc;

pub struct AppData {
	pub chat: Chat,
	pub diagram_cache: DiagramCache,
	pub key_manager: KeyManager,
	pub config_manager: ConfigManager,
	pub mcp_config_manager: McpConfigManager,
	pub mcp_stdio_manager: Arc<McpStdioManager>,
	pub mcp_http_manager: Arc<McpHttpManager>,
}

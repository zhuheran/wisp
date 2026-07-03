use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use wisp_db::chat::Chat;
use wisp_configs::ConfigManager;
use wisp_mcp::McpConfigManager;
use wisp_mcp::McpStdioManager;
use wisp_mcp::McpHttpManager;
use wisp_tool_registry::ToolRegistry;

use crate::cache::DiagramCache;
use wisp_keyring::KeyManager;

pub struct AppData {
    pub chat: Chat,
    pub diagram_cache: DiagramCache,
    pub key_manager: KeyManager,
    pub config_manager: Arc<ConfigManager>,
    pub mcp_config_manager: McpConfigManager,
    pub mcp_stdio_manager: Arc<McpStdioManager>,
    pub mcp_http_manager: Arc<McpHttpManager>,
    pub tool_registry: Arc<ToolRegistry>,
    pub unlocked_pals: HashMap<String, HashSet<String>>,
}

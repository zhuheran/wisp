use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use wisp_configs::ConfigManager;
use wisp_db::chat::Chat;
use wisp_mcp::McpConfigManager;
use wisp_mcp::McpHttpManager;
use wisp_mcp::McpStdioManager;
use wisp_software_tools::SoftwareToolRegistry;
use wisp_tool_registry::ToolRegistry;

use crate::cache::DiagramCache;

pub struct AppData {
    pub chat: Chat,
    pub diagram_cache: DiagramCache,
    pub config_manager: Arc<ConfigManager>,
    pub mcp_config_manager: McpConfigManager,
    pub mcp_stdio_manager: Arc<McpStdioManager>,
    pub mcp_http_manager: Arc<McpHttpManager>,
    pub tool_registry: Arc<ToolRegistry>,
    pub software_registry: Arc<SoftwareToolRegistry>,
    pub unlocked_pals: HashMap<String, HashSet<String>>,
    /// Installed Agent Skills (loaded from the skills directories).
    pub skills: Vec<wisp_skills::Skill>,
    /// Names of enabled skills. Only enabled skills are advertised in the
    /// L1 metadata and exposed via the `load_skill` tool's enum.
    pub enabled_skills: std::collections::HashSet<String>,
}

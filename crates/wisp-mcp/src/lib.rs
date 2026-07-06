pub mod config;
pub mod http;
pub mod mcp_handler;
pub mod stdio;
pub mod types;

pub use config::McpConfigManager;
pub use http::{McpHttpClient, McpHttpManager};
pub use mcp_handler::{register_mcp_tools, unregister_mcp_server, McpToolHandler};
pub use stdio::{McpStdioClient, McpStdioManager};
pub use types::*;

// Re-export from wisp-tool-registry for backward compatibility
pub use wisp_tool_registry::{
    registered_name, ToolAnnotations, ToolDefinition, ToolHandler, ToolRegistry,
};

// Re-export from wisp-common for backward compatibility
pub use wisp_common::{ToolContent, ToolError, ToolResult};

// Re-export from wisp-configs for backward compatibility
pub use wisp_configs::{ConversationLoopConfig, PipelineConfig};

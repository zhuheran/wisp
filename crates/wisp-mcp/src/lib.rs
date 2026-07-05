pub mod types;
pub mod config;
pub mod http;
pub mod stdio;
pub mod mcp_handler;

pub use types::*;
pub use config::McpConfigManager;
pub use http::{McpHttpClient, McpHttpManager};
pub use stdio::{McpStdioClient, McpStdioManager};
pub use mcp_handler::{McpToolHandler, register_mcp_tools, unregister_mcp_server};

// Re-export from wisp-tool-registry for backward compatibility
pub use wisp_tool_registry::{registered_name, ToolAnnotations, ToolDefinition, ToolHandler, ToolRegistry};

// Re-export from wisp-common for backward compatibility
pub use wisp_common::{ToolContent, ToolError, ToolResult};

// Re-export from wisp-configs for backward compatibility
pub use wisp_configs::{PipelineConfig, ConversationLoopConfig};

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wisp_common::{ToolError, ToolResult};
use wisp_tool_registry::{
    registered_name, ToolAnnotations, ToolDefinition, ToolHandler, ToolRegistry,
};

use crate::http::McpHttpManager;
use crate::stdio::McpStdioManager;
use crate::types::{NormalizedTool, TransportConfig};

pub struct McpToolHandler {
    server_id: String,
    original_name: String,
    stdio_manager: Arc<McpStdioManager>,
    http_manager: Arc<McpHttpManager>,
    transport: TransportConfig,
}

impl McpToolHandler {
    pub fn new(
        server_id: String,
        original_name: String,
        stdio_manager: Arc<McpStdioManager>,
        http_manager: Arc<McpHttpManager>,
        transport: TransportConfig,
    ) -> Self {
        McpToolHandler { server_id, original_name, stdio_manager, http_manager, transport }
    }
}

#[async_trait]
impl ToolHandler for McpToolHandler {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let mcp_args = match args {
            Value::Object(_) => Some(args),
            Value::Null => None,
            other => Some(other),
        };

        let raw = match &self.transport {
            TransportConfig::Stdio { .. } => self
                .stdio_manager
                .call_tool(&self.server_id, &self.original_name, mcp_args)
                .await
                .map_err(|e| {
                    ToolError::ExecutionFailed(format!(
                        "MCP stdio, server '{}': {e}",
                        self.server_id
                    ))
                })?,
            TransportConfig::Sse { .. } | TransportConfig::Http { .. } => self
                .http_manager
                .call_tool(&self.server_id, &self.original_name, mcp_args)
                .await
                .map_err(|e| {
                    ToolError::ExecutionFailed(format!(
                        "MCP http, server '{}': {e}",
                        self.server_id
                    ))
                })?,
        };

        Ok(ToolResult::from_mcp_response(raw))
    }
}

pub fn register_mcp_tools(
    registry: &ToolRegistry,
    server_id: &str,
    tools: &[NormalizedTool],
    transport: &TransportConfig,
    stdio_manager: Arc<McpStdioManager>,
    http_manager: Arc<McpHttpManager>,
) {
    for tool in tools {
        let name = registered_name(server_id, &tool.name);
        let definition = ToolDefinition {
            name: name.clone(),
            description: tool.description.clone(),
            input_schema: tool.input_schema.clone(),
            annotations: tool.annotations.as_ref().map(|a| ToolAnnotations {
                title: a.title.clone(),
                read_only_hint: a.read_only_hint,
                destructive_hint: a.destructive_hint,
                idempotent_hint: a.idempotent_hint,
                open_world_hint: a.open_world_hint,
            }),
            metadata: std::collections::HashMap::from([
                ("provider".to_string(), Value::String("mcp".to_string())),
                ("server_id".to_string(), Value::String(server_id.to_string())),
                ("original_name".to_string(), Value::String(tool.name.clone())),
            ]),
            requires_confirmation: false,
        };

        let handler = Arc::new(McpToolHandler::new(
            server_id.to_string(),
            tool.name.clone(),
            Arc::clone(&stdio_manager),
            Arc::clone(&http_manager),
            transport.clone(),
        ));

        registry.register(definition, handler, vec![]);
    }
}

pub fn unregister_mcp_server(registry: &ToolRegistry, server_id: &str) -> Vec<String> {
    registry.unregister_by_metadata("server_id", server_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_normalized_tool(name: &str, server_id: &str) -> NormalizedTool {
        NormalizedTool {
            name: name.to_string(),
            server_id: server_id.to_string(),
            qualified_name: format!("{}:{}", server_id, name),
            description: Some(format!("The {name} tool")),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
            annotations: None,
        }
    }

    fn make_stdio_transport() -> TransportConfig {
        TransportConfig::Stdio {
            command: "echo".to_string(),
            args: vec![],
            env: HashMap::new(),
            cwd: None,
        }
    }

    #[test]
    fn test_register_mcp_tools_creates_correct_metadata() {
        let registry = ToolRegistry::new();
        let stdio_mgr = Arc::new(McpStdioManager::new());
        let http_mgr = Arc::new(McpHttpManager::new());
        let tools = vec![make_normalized_tool("search", "tavily")];

        register_mcp_tools(
            &registry,
            "tavily",
            &tools,
            &make_stdio_transport(),
            stdio_mgr,
            http_mgr,
        );

        let def = registry
            .get_tool("mcp_tavily_search")
            .expect("tool should be registered");
        assert_eq!(def.metadata.get("provider").and_then(|v| v.as_str()), Some("mcp"));
        assert_eq!(def.metadata.get("server_id").and_then(|v| v.as_str()), Some("tavily"));
        assert_eq!(
            def.metadata.get("original_name").and_then(|v| v.as_str()),
            Some("search")
        );
        assert!(!def.requires_confirmation);
    }

    #[test]
    fn test_register_mcp_tools_enables_by_default() {
        let registry = ToolRegistry::new();
        let stdio_mgr = Arc::new(McpStdioManager::new());
        let http_mgr = Arc::new(McpHttpManager::new());
        register_mcp_tools(
            &registry,
            "srv",
            &[make_normalized_tool("x", "srv")],
            &make_stdio_transport(),
            stdio_mgr,
            http_mgr,
        );

        assert!(registry.enabled_set().contains("mcp_srv_x"));
    }

    #[test]
    fn test_unregister_mcp_server_removes_tools() {
        let registry = ToolRegistry::new();
        let stdio_mgr = Arc::new(McpStdioManager::new());
        let http_mgr = Arc::new(McpHttpManager::new());
        register_mcp_tools(
            &registry,
            "srv",
            &[
                make_normalized_tool("a", "srv"),
                make_normalized_tool("b", "srv"),
            ],
            &make_stdio_transport(),
            stdio_mgr,
            http_mgr,
        );

        let removed = unregister_mcp_server(&registry, "srv");
        assert_eq!(removed.len(), 2);
        assert!(registry.list_tools().is_empty());
    }
}

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::Value;

use crate::mcp::types::{NormalizedTool, TransportConfig};
use crate::mcp_http::McpHttpManager;
use crate::mcp_stdio::McpStdioManager;

use super::types::{
    registered_name, ToolAnnotations, ToolContent, ToolDefinition, ToolError, ToolResult,
};

/// Internal entry for a registered tool.
#[derive(Clone)]
struct McpToolEntry {
    definition: ToolDefinition,
    /// The MCP server that owns this tool.
    server_id: String,
    /// The original tool name as reported by the MCP server.
    original_name: String,
    /// The transport used to reach this server.
    transport: TransportConfig,
}

/// Central registry for all tools.
///
/// Tools are registered with an `mcp_` prefix name and can be executed
/// without the caller knowing about transports or server IDs.
///
/// Mutable operations (register/unregister) use an internal `Mutex` so the
/// `ToolRegistry` can be shared behind an `Arc` across async boundaries.
///
/// # Example
///
/// ```ignore
/// let reg = ToolRegistry::new(stdio_manager, http_manager);
/// reg.register_server("tavily", &tools, &TransportConfig::Http { .. });
/// let result = reg.execute("mcp_tavily_search", args).await?;
/// ```
pub struct ToolRegistry {
    /// Internal mutable state.
    inner: std::sync::Mutex<Inner>,
    /// MCP stdio execution (opaque to callers).
    stdio_manager: Arc<McpStdioManager>,
    /// MCP HTTP/SSE execution (opaque to callers).
    http_manager: Arc<McpHttpManager>,
}

struct Inner {
    /// Registered name → tool entry.
    entries: HashMap<String, McpToolEntry>,
    /// Set of enabled registered names.
    enabled: HashSet<String>,
}

impl ToolRegistry {
    pub fn new(
        stdio_manager: Arc<McpStdioManager>,
        http_manager: Arc<McpHttpManager>,
    ) -> Self {
        ToolRegistry {
            inner: std::sync::Mutex::new(Inner {
                entries: HashMap::new(),
                enabled: HashSet::new(),
            }),
            stdio_manager,
            http_manager,
        }
    }

    // ── Registration ───────────────────────────────────────────

    /// Register all tools from an MCP server.
    ///
    /// Each tool gets a name of the form `mcp_{server_id}_{tool_name}`.
    /// Newly registered tools are **enabled by default**.
    pub fn register_server(
        &self,
        server_id: &str,
        tools: &[NormalizedTool],
        transport: &TransportConfig,
    ) {
        let mut inner = self.inner.lock().unwrap();
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
                metadata: HashMap::from([
                    ("provider".to_string(), Value::String("mcp".to_string())),
                    ("server_id".to_string(), Value::String(server_id.to_string())),
                    ("original_name".to_string(), Value::String(tool.name.clone())),
                ]),
                enabled: true,
            };

            inner.entries.insert(
                name.clone(),
                McpToolEntry {
                    definition,
                    server_id: server_id.to_string(),
                    original_name: tool.name.clone(),
                    transport: transport.clone(),
                },
            );
            inner.enabled.insert(name);
        }
    }

    /// Remove all tools belonging to a given server.
    ///
    /// Returns the list of removed registered names.
    pub fn unregister_server(&self, server_id: &str) -> Vec<String> {
        let mut inner = self.inner.lock().unwrap();
        let removed: Vec<String> = inner
            .entries
            .iter()
            .filter(|(_, entry)| entry.server_id == server_id)
            .map(|(name, _)| name.clone())
            .collect();

        for name in &removed {
            inner.entries.remove(name);
            inner.enabled.remove(name);
        }

        removed
    }

    // ── Query ──────────────────────────────────────────────────

    /// Returns all registered tool definitions (regardless of enabled state).
    pub fn list_tools(&self) -> Vec<ToolDefinition> {
        let inner = self.inner.lock().unwrap();
        inner.entries.values().map(|e| e.definition.clone()).collect()
    }

    /// Returns only the enabled tool definitions.
    pub fn list_enabled_tools(&self) -> Vec<ToolDefinition> {
        let inner = self.inner.lock().unwrap();
        inner
            .entries
            .values()
            .filter(|e| inner.enabled.contains(&e.definition.name))
            .map(|e| e.definition.clone())
            .collect()
    }

    /// Look up a tool by its registered name.
    pub fn get_tool(&self, name: &str) -> Option<ToolDefinition> {
        let inner = self.inner.lock().unwrap();
        inner.entries.get(name).map(|e| e.definition.clone())
    }

    /// Returns a copy of the set of enabled registered names.
    pub fn enabled_set(&self) -> HashSet<String> {
        let inner = self.inner.lock().unwrap();
        inner.enabled.clone()
    }

    /// Overwrite the enabled set with the given names.
    ///
    /// Names that are not registered are silently ignored.
    pub fn set_enabled(&self, names: HashSet<String>) {
        let mut inner = self.inner.lock().unwrap();
        let available: HashSet<String> = inner.entries.keys().cloned().collect();
        inner.enabled = names
            .into_iter()
            .filter(|name| available.contains(name))
            .collect();
    }

    /// Enable or disable a specific tool by its registered name.
    /// Silently ignored if the name is not registered.
    pub fn set_tool_enabled(&self, name: &str, enabled: bool) {
        let mut inner = self.inner.lock().unwrap();
        if inner.entries.contains_key(name) {
            if enabled {
                inner.enabled.insert(name.to_string());
            } else {
                inner.enabled.remove(name);
            }
        }
    }

    /// Enable or disable all tools from a server.
    pub fn set_server_enabled(&self, server_id: &str, enabled: bool) {
        let mut inner = self.inner.lock().unwrap();
        let matching: Vec<String> = inner
            .entries
            .iter()
            .filter(|(_, entry)| entry.server_id == server_id)
            .map(|(name, _)| name.clone())
            .collect();
        for name in matching {
            if enabled {
                inner.enabled.insert(name);
            } else {
                inner.enabled.remove(&name);
            }
        }
    }

    // ── Execution ──────────────────────────────────────────────

    /// Execute a tool by its registered name.
    ///
    /// Returns a future — the caller does not need to know about transports.
    pub async fn execute(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        let entry = {
            let inner = self.inner.lock().unwrap();
            inner
                .entries
                .get(name)
                .cloned()
                .ok_or_else(|| ToolError::NotFound(name.to_string()))?
        };

        let mcp_args = match args {
            Value::Object(_) => Some(args),
            Value::Null => None,
            other => Some(other),
        };

        let raw = match &entry.transport {
            TransportConfig::Stdio { .. } => self
                .stdio_manager
                .call_tool(&entry.server_id, &entry.original_name, mcp_args)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!(
                    "tool '{name}' (MCP stdio, server '{}'): {e}", entry.server_id
                )))?,
            TransportConfig::Sse { .. } | TransportConfig::Http { .. } => self
                .http_manager
                .call_tool(&entry.server_id, &entry.original_name, mcp_args)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!(
                    "tool '{name}' (MCP http, server '{}'): {e}", entry.server_id
                )))?,
        };

        Ok(ToolResult::from_mcp_response(raw))
    }

    // ── Prompt generation ──────────────────────────────────────

    /// Build an AI-readable text prompt listing all enabled tools.
    ///
    /// Returns an empty string when no tools are enabled.
    pub fn build_tools_prompt(&self) -> String {
        let mut tools = self.list_enabled_tools();
        if tools.is_empty() {
            return String::new();
        }
        tools.sort_by(|a, b| a.name.cmp(&b.name));

        let mut lines = Vec::new();
        lines.push("## Available Tools".to_string());
        lines.push(String::new());
        lines.push(
            "You have access to the following tools. Use them via <|tool_calls|> when appropriate."
                .to_string(),
        );
        lines.push(String::new());

        for tool in &tools {
            let desc = tool
                .description
                .as_deref()
                .unwrap_or("No description");
            lines.push(format!("- **{}**: {desc}", tool.name));

            if let Some(props) = tool
                .input_schema
                .get("properties")
                .and_then(|v| v.as_object())
            {
                let mut prop_names: Vec<&String> = props.keys().collect();
                prop_names.sort();
                for prop_name in prop_names {
                    let prop = &props[prop_name];
                    let desc = prop
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let type_str = prop
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("string");
                    lines.push(format!("  - `{prop_name}` ({type_str}): {desc}"));
                }
            }
        }

        lines.push(String::new());
        lines.push("Call tools by wrapping a JSON array in `<|tool_calls|>` tags:".to_string());
        lines.push("<|tool_calls|>".to_string());
        lines.push(
            r#"[{"name":"tool_name","arguments":{"param1":"value1"}}]"#.to_string(),
        );
        lines.push("<|/tool_calls|>".to_string());

        lines.join("\n")
    }

    /// Build the native `ChatCompletionTool` list for OpenAI-style APIs.
    pub fn build_provider_tools(&self) -> Vec<crate::mcp::types::ToolCall> {
        // Return a simplified tool list; actual ChatCompletionTool construction
        // happens in the conversation layer where async_openai is available.
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::types::NormalizedTool;

    // ── Helpers ────────────────────────────────────────────────

    fn make_tool(name: &str, server_id: &str) -> NormalizedTool {
        NormalizedTool {
            name: name.to_string(),
            server_id: server_id.to_string(),
            qualified_name: format!("{server_id}:{name}"),
            description: Some(format!("The {name} tool")),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "q": {
                        "type": "string",
                        "description": "Search query"
                    }
                },
                "required": ["q"]
            }),
            annotations: None,
        }
    }

    fn make_transport_stdio() -> TransportConfig {
        TransportConfig::Stdio {
            command: "echo".to_string(),
            args: vec![],
            env: HashMap::new(),
            cwd: None,
        }
    }

    fn make_transport_http() -> TransportConfig {
        TransportConfig::Http {
            url: "http://localhost:9999".to_string(),
            headers: HashMap::new(),
            session_id: None,
        }
    }

    fn make_registry() -> ToolRegistry {
        let stdio_mgr = Arc::new(McpStdioManager::new());
        let http_mgr = Arc::new(McpHttpManager::new());
        ToolRegistry::new(stdio_mgr, http_mgr)
    }

    // ── register_server ────────────────────────────────────────

    #[test]
    fn test_register_server_adds_tools_with_mcp_prefix() {
        let mut reg = make_registry();
        let tools = vec![make_tool("search", "tavily")];
        reg.register_server("tavily", &tools, &make_transport_stdio());

        let def = reg.get_tool("mcp_tavily_search").expect("tool should be registered");
        assert_eq!(def.name, "mcp_tavily_search");
        assert!(def.enabled);
    }

    #[test]
    fn test_register_server_multiple_tools() {
        let mut reg = make_registry();
        let tools = vec![
            make_tool("search", "srv"),
            make_tool("read", "srv"),
        ];
        reg.register_server("srv", &tools, &make_transport_stdio());

        assert_eq!(reg.list_tools().len(), 2);
        assert!(reg.get_tool("mcp_srv_search").is_some());
        assert!(reg.get_tool("mcp_srv_read").is_some());
    }

    #[test]
    fn test_register_server_same_name_different_servers() {
        let mut reg = make_registry();
        reg.register_server(
            "a",
            &[make_tool("search", "a")],
            &make_transport_stdio(),
        );
        reg.register_server(
            "b",
            &[make_tool("search", "b")],
            &make_transport_http(),
        );

        let a = reg.get_tool("mcp_a_search").expect("a's tool");
        let b = reg.get_tool("mcp_b_search").expect("b's tool");
        assert_eq!(a.name, "mcp_a_search");
        assert_eq!(b.name, "mcp_b_search");
    }

    #[test]
    fn test_register_server_new_tools_are_enabled_by_default() {
        let mut reg = make_registry();
        reg.register_server("s", &[make_tool("x", "s")], &make_transport_stdio());

        assert!(reg.enabled_set().contains("mcp_s_x"));
    }

    #[test]
    fn test_register_server_empty_tool_list() {
        let mut reg = make_registry();
        reg.register_server("empty", &[], &make_transport_stdio());
        assert!(reg.list_tools().is_empty());
    }

    // ── unregister_server ──────────────────────────────────────

    #[test]
    fn test_unregister_server_removes_all_tools() {
        let mut reg = make_registry();
        reg.register_server("srv", &[make_tool("a", "srv"), make_tool("b", "srv")], &make_transport_stdio());
        assert_eq!(reg.list_tools().len(), 2);

        let removed = reg.unregister_server("srv");
        assert_eq!(removed.len(), 2);
        assert!(reg.list_tools().is_empty());
    }

    #[test]
    fn test_unregister_server_removes_tools_from_enabled_set() {
        let mut reg = make_registry();
        reg.register_server("s", &[make_tool("x", "s")], &make_transport_stdio());
        assert!(reg.enabled_set().contains("mcp_s_x"));

        reg.unregister_server("s");
        assert!(!reg.enabled_set().contains("mcp_s_x"));
    }

    #[test]
    fn test_unregister_server_unknown_server_does_nothing() {
        let mut reg = make_registry();
        reg.register_server("real", &[make_tool("t", "real")], &make_transport_stdio());
        let removed = reg.unregister_server("ghost");
        assert!(removed.is_empty());
        assert_eq!(reg.list_tools().len(), 1);
    }

    #[test]
    fn test_register_after_unregister() {
        let mut reg = make_registry();
        reg.register_server("s", &[make_tool("x", "s")], &make_transport_stdio());
        reg.unregister_server("s");
        assert!(reg.list_tools().is_empty());

        reg.register_server("s", &[make_tool("x", "s")], &make_transport_stdio());
        assert!(reg.get_tool("mcp_s_x").is_some());
    }

    // ── get_tool / list_tools / list_enabled_tools ─────────────

    #[test]
    fn test_get_tool_returns_none_for_unknown() {
        let reg = make_registry();
        assert!(reg.get_tool("nonexistent").is_none());
    }

    #[test]
    fn test_get_tool_returns_some_for_registered() {
        let mut reg = make_registry();
        reg.register_server("s", &[make_tool("t", "s")], &make_transport_stdio());
        assert!(reg.get_tool("mcp_s_t").is_some());
    }

    #[test]
    fn test_list_tools_returns_all() {
        let mut reg = make_registry();
        reg.register_server(
            "s",
            &[make_tool("a", "s"), make_tool("b", "s")],
            &make_transport_stdio(),
        );
        assert_eq!(reg.list_tools().len(), 2);
    }

    #[test]
    fn test_list_tools_empty_when_nothing_registered() {
        let reg = make_registry();
        assert!(reg.list_tools().is_empty());
    }

    #[test]
    fn test_list_enabled_tools_filters_disabled() {
        let mut reg = make_registry();
        reg.register_server(
            "s",
            &[make_tool("a", "s"), make_tool("b", "s")],
            &make_transport_stdio(),
        );
        // Disable one
        reg.set_tool_enabled("mcp_s_a", false);
        let enabled = reg.list_enabled_tools();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].name, "mcp_s_b");
    }

    #[test]
    fn test_list_enabled_tools_empty_when_all_disabled() {
        let mut reg = make_registry();
        reg.register_server("s", &[make_tool("t", "s")], &make_transport_stdio());
        reg.set_tool_enabled("mcp_s_t", false);
        assert!(reg.list_enabled_tools().is_empty());
    }

    // ── set_enabled / set_tool_enabled / set_server_enabled ────

    #[test]
    fn test_set_enabled_overwrites_with_filtered_set() {
        let mut reg = make_registry();
        reg.register_server(
            "s",
            &[make_tool("a", "s"), make_tool("b", "s")],
            &make_transport_stdio(),
        );
        let mut names = HashSet::new();
        names.insert("mcp_s_a".to_string());
        names.insert("nonexistent".to_string()); // should be ignored
        reg.set_enabled(names);

        assert!(reg.enabled_set().contains("mcp_s_a"));
        assert!(!reg.enabled_set().contains("mcp_s_b"));
    }

    #[test]
    fn test_set_tool_enabled_false_disables() {
        let mut reg = make_registry();
        reg.register_server("s", &[make_tool("t", "s")], &make_transport_stdio());
        reg.set_tool_enabled("mcp_s_t", false);
        assert!(!reg.enabled_set().contains("mcp_s_t"));
    }

    #[test]
    fn test_set_tool_enabled_unknown_name_does_nothing() {
        let mut reg = make_registry();
        reg.set_tool_enabled("ghost", false); // should not panic
        assert!(reg.enabled_set().is_empty());
    }

    #[test]
    fn test_set_server_enabled_disables_all_server_tools() {
        let mut reg = make_registry();
        reg.register_server(
            "s",
            &[make_tool("a", "s"), make_tool("b", "s")],
            &make_transport_stdio(),
        );
        reg.set_server_enabled("s", false);
        assert!(!reg.enabled_set().contains("mcp_s_a"));
        assert!(!reg.enabled_set().contains("mcp_s_b"));
    }

    #[test]
    fn test_set_server_enabled_unknown_server_does_nothing() {
        let mut reg = make_registry();
        reg.register_server("s", &[make_tool("t", "s")], &make_transport_stdio());
        reg.set_server_enabled("ghost", false);
        assert!(reg.enabled_set().contains("mcp_s_t"));
    }

    #[test]
    fn test_set_server_enabled_reenables() {
        let mut reg = make_registry();
        reg.register_server("s", &[make_tool("t", "s")], &make_transport_stdio());
        reg.set_tool_enabled("mcp_s_t", false);
        reg.set_server_enabled("s", true);
        assert!(reg.enabled_set().contains("mcp_s_t"));
    }

    // ── execute (error paths only) ─────────────────────────────

    #[tokio::test]
    async fn test_execute_unknown_tool_returns_not_found() {
        let reg = make_registry();
        let err = reg.execute("ghost", serde_json::Value::Null).await;
        assert!(matches!(err, Err(ToolError::NotFound(n)) if n == "ghost"));
    }

    // ── build_tools_prompt ─────────────────────────────────────

    #[test]
    fn test_build_tools_prompt_returns_formatted_text() {
        let mut reg = make_registry();
        reg.register_server("s", &[make_tool("t", "s")], &make_transport_stdio());
        let prompt = reg.build_tools_prompt();

        assert!(prompt.contains("## Available Tools"));
        assert!(prompt.contains("**mcp_s_t**"));
        assert!(prompt.contains("The t tool"));
        assert!(prompt.contains("`q` (string)"));
        assert!(prompt.contains("<|tool_calls|>"));
    }

    #[test]
    fn test_build_tools_prompt_empty_when_no_tools() {
        let reg = make_registry();
        assert!(reg.build_tools_prompt().is_empty());
    }

    #[test]
    fn test_build_tools_prompt_empty_when_all_disabled() {
        let mut reg = make_registry();
        reg.register_server("s", &[make_tool("t", "s")], &make_transport_stdio());
        reg.set_tool_enabled("mcp_s_t", false);
        assert!(reg.build_tools_prompt().is_empty());
    }

    #[test]
    fn test_build_tools_prompt_disabled_tools_not_included() {
        let mut reg = make_registry();
        reg.register_server(
            "s",
            &[make_tool("a", "s"), make_tool("b", "s")],
            &make_transport_stdio(),
        );
        reg.set_tool_enabled("mcp_s_a", false);
        let prompt = reg.build_tools_prompt();
        assert!(!prompt.contains("mcp_s_a"));
        assert!(prompt.contains("mcp_s_b"));
    }

    #[test]
    fn test_build_tools_prompt_tool_without_properties() {
        let mut reg = make_registry();
        let mut tool = make_tool("no_params", "s");
        tool.input_schema = serde_json::json!({ "type": "object" });
        reg.register_server("s", &[tool], &make_transport_stdio());
        let prompt = reg.build_tools_prompt();
        assert!(prompt.contains("**mcp_s_no_params**"));
        // Should not crash when properties are missing
    }

    #[test]
    fn test_build_tools_prompt_multiple_tools_sorted_by_name() {
        let mut reg = make_registry();
        reg.register_server(
            "s",
            &[make_tool("z_tool", "s"), make_tool("a_tool", "s")],
            &make_transport_stdio(),
        );
        let prompt = reg.build_tools_prompt();
        let bullet_lines: Vec<&str> = prompt
            .lines()
            .filter(|line| line.starts_with("- **mcp_s_"))
            .collect();
        assert_eq!(bullet_lines.len(), 2);
        assert!(bullet_lines[0].contains("mcp_s_a_tool"));
        assert!(bullet_lines[1].contains("mcp_s_z_tool"));
    }
}

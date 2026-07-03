use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use super::handler::ToolHandler;
use super::types::ToolDefinition;
use wisp_common::{ToolError, ToolResult};

struct ToolEntry {
    definition: ToolDefinition,
    handler: Arc<dyn ToolHandler>,
}

struct Inner {
    entries: HashMap<String, ToolEntry>,
    enabled: HashSet<String>,
    allowed_pals: HashMap<String, Vec<String>>,
}

pub struct ToolRegistry {
    inner: Mutex<Inner>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        ToolRegistry {
            inner: Mutex::new(Inner {
                entries: HashMap::new(),
                enabled: HashSet::new(),
                allowed_pals: HashMap::new(),
            }),
        }
    }

    pub fn register(
        &self,
        definition: ToolDefinition,
        handler: Arc<dyn ToolHandler>,
        allowed_pals: Vec<String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        let name = definition.name.clone();
        inner.entries.insert(name.clone(), ToolEntry { definition, handler });
        inner.enabled.insert(name.clone());
        if !allowed_pals.is_empty() {
            inner.allowed_pals.insert(name, allowed_pals);
        }
    }

    pub fn unregister(&self, name: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.entries.remove(name);
        inner.enabled.remove(name);
        inner.allowed_pals.remove(name);
    }

    /// Remove all tools where `metadata[key] == value`.
    /// Returns the list of removed registered names.
    pub fn unregister_by_metadata(&self, key: &str, value: &str) -> Vec<String> {
        let mut inner = self.inner.lock().unwrap();
        let removed: Vec<String> = inner
            .entries
            .iter()
            .filter(|(_, e)| {
                e.definition
                    .metadata
                    .get(key)
                    .and_then(|v| v.as_str())
                    .map(|v| v == value)
                    .unwrap_or(false)
            })
            .map(|(name, _)| name.clone())
            .collect();

        for name in &removed {
            inner.entries.remove(name);
            inner.enabled.remove(name);
            inner.allowed_pals.remove(name);
        }
        removed
    }

    pub fn list_tools(&self) -> Vec<ToolDefinition> {
        let inner = self.inner.lock().unwrap();
        inner.entries.values().map(|e| e.definition.clone()).collect()
    }

    pub fn list_enabled_tools(&self) -> Vec<ToolDefinition> {
        let inner = self.inner.lock().unwrap();
        inner
            .entries
            .values()
            .filter(|e| inner.enabled.contains(&e.definition.name))
            .map(|e| e.definition.clone())
            .collect()
    }

    pub fn get_tool(&self, name: &str) -> Option<ToolDefinition> {
        let inner = self.inner.lock().unwrap();
        inner.entries.get(name).map(|e| e.definition.clone())
    }

    pub fn enabled_set(&self) -> HashSet<String> {
        let inner = self.inner.lock().unwrap();
        inner.enabled.clone()
    }

    pub fn set_enabled(&self, names: HashSet<String>) {
        let mut inner = self.inner.lock().unwrap();
        let available: HashSet<String> = inner.entries.keys().cloned().collect();
        inner.enabled = names
            .into_iter()
            .filter(|name| available.contains(name))
            .collect();
    }

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

    pub fn set_tool_allowed_pals(&self, name: &str, pal_ids: Vec<String>) {
        let mut inner = self.inner.lock().unwrap();
        if inner.entries.contains_key(name) {
            if pal_ids.is_empty() {
                inner.allowed_pals.remove(name);
            } else {
                inner.allowed_pals.insert(name.to_string(), pal_ids);
            }
        }
    }

    pub async fn execute(
        &self,
        name: &str,
        args: Value,
        pal_id: Option<&str>,
    ) -> Result<ToolResult, ToolError> {
        let handler: Arc<dyn ToolHandler> = {
            let inner = self.inner.lock().unwrap();
            let entry = inner
                .entries
                .get(name)
                .ok_or_else(|| ToolError::NotFound(name.to_string()))?;

            if !inner.enabled.contains(name) {
                return Err(ToolError::ExecutionFailed(format!(
                    "tool '{name}' is disabled"
                )));
            }

            if let Some(allowed) = inner.allowed_pals.get(name) {
                if let Some(pid) = pal_id {
                    if !allowed.iter().any(|p| p == pid) {
                        return Err(ToolError::ExecutionFailed(format!(
                            "pal '{pid}' is not permitted to use tool '{name}'"
                        )));
                    }
                }
            }

            Arc::clone(&entry.handler)
        };

        handler.execute(args).await
    }

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
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct EchoHandler;

    #[async_trait]
    impl ToolHandler for EchoHandler {
        async fn execute(&self, _args: Value) -> Result<ToolResult, ToolError> {
            Ok(ToolResult {
                content: vec![wisp_common::ToolContent::Text { text: "ok".to_string() }],
                is_error: false,
            })
        }
    }

    fn make_definition(name: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            description: Some(format!("The {name} tool")),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "q": { "type": "string", "description": "query" } },
                "required": ["q"]
            }),
            annotations: None,
            metadata: HashMap::new(),
            requires_confirmation: false,
        }
    }

    #[test]
    fn test_register_adds_tool() {
        let reg = ToolRegistry::new();
        reg.register(make_definition("test_tool"), Arc::new(EchoHandler), vec![]);
        assert!(reg.get_tool("test_tool").is_some());
        assert!(reg.enabled_set().contains("test_tool"));
    }

    #[test]
    fn test_unregister_removes_tool() {
        let reg = ToolRegistry::new();
        reg.register(make_definition("test_tool"), Arc::new(EchoHandler), vec![]);
        reg.unregister("test_tool");
        assert!(reg.get_tool("test_tool").is_none());
        assert!(!reg.enabled_set().contains("test_tool"));
    }

    #[test]
    fn test_unregister_by_metadata() {
        let reg = ToolRegistry::new();
        let mut def = make_definition("mcp_srv_a");
        def.metadata.insert("server_id".to_string(), Value::String("srv".to_string()));
        reg.register(def, Arc::new(EchoHandler), vec![]);
        let mut def2 = make_definition("mcp_srv_b");
        def2.metadata.insert("server_id".to_string(), Value::String("srv".to_string()));
        reg.register(def2, Arc::new(EchoHandler), vec![]);

        let removed = reg.unregister_by_metadata("server_id", "srv");
        assert_eq!(removed.len(), 2);
        assert!(reg.list_tools().is_empty());
    }

    #[test]
    fn test_list_enabled_tools_filters_disabled() {
        let reg = ToolRegistry::new();
        reg.register(make_definition("a"), Arc::new(EchoHandler), vec![]);
        reg.register(make_definition("b"), Arc::new(EchoHandler), vec![]);
        reg.set_tool_enabled("a", false);
        let enabled = reg.list_enabled_tools();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].name, "b");
    }

    #[test]
    fn test_set_tool_allowed_pals() {
        let reg = ToolRegistry::new();
        reg.register(make_definition("t"), Arc::new(EchoHandler), vec!["admin".to_string()]);
        let inner = reg.inner.lock().unwrap();
        assert_eq!(
            inner.allowed_pals.get("t"),
            Some(&vec!["admin".to_string()])
        );
    }

    #[tokio::test]
    async fn test_execute_unknown_returns_not_found() {
        let reg = ToolRegistry::new();
        let err = reg.execute("ghost", Value::Null, None).await;
        assert!(matches!(err, Err(ToolError::NotFound(n)) if n == "ghost"));
    }

    #[tokio::test]
    async fn test_execute_disabled_returns_error() {
        let reg = ToolRegistry::new();
        reg.register(make_definition("t"), Arc::new(EchoHandler), vec![]);
        reg.set_tool_enabled("t", false);
        let err = reg.execute("t", Value::Null, None).await;
        assert!(matches!(err, Err(ToolError::ExecutionFailed(_))));
    }

    #[tokio::test]
    async fn test_execute_pal_not_permitted() {
        let reg = ToolRegistry::new();
        reg.register(make_definition("t"), Arc::new(EchoHandler), vec!["admin".to_string()]);
        let err = reg.execute("t", Value::Null, Some("guest")).await;
        assert!(matches!(err, Err(ToolError::ExecutionFailed(_))));
    }

    #[tokio::test]
    async fn test_execute_pal_permitted_succeeds() {
        let reg = ToolRegistry::new();
        reg.register(make_definition("t"), Arc::new(EchoHandler), vec!["admin".to_string()]);
        let result = reg.execute("t", Value::Null, Some("admin")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_no_pal_restriction_succeeds() {
        let reg = ToolRegistry::new();
        reg.register(make_definition("t"), Arc::new(EchoHandler), vec![]);
        let result = reg.execute("t", Value::Null, Some("anyone")).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_tools_prompt_returns_formatted_text() {
        let reg = ToolRegistry::new();
        reg.register(make_definition("test_tool"), Arc::new(EchoHandler), vec![]);
        let prompt = reg.build_tools_prompt();
        assert!(prompt.contains("## Available Tools"));
        assert!(prompt.contains("**test_tool**"));
        assert!(prompt.contains("`q` (string)"));
    }

    #[test]
    fn test_build_tools_prompt_empty_when_no_tools() {
        let reg = ToolRegistry::new();
        assert!(reg.build_tools_prompt().is_empty());
    }
}

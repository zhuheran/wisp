# Software Tools Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract `ToolRegistry` into a new `wisp-tool-registry` crate, add a `wisp-software-tools` crate for native tools, and wire them into the app — all with zero behavior change for existing MCP tools.

**Architecture:** `ToolRegistry` moves to `wisp-tool-registry` and dispatches through a `ToolHandler` trait. MCP tools implement `ToolHandler` via `McpToolHandler`. Native tools implement `NativeTool` (which extends `ToolHandler`). Shared result types (`ToolContent`, `ToolResult`, `ToolError`) move to `wisp-common`.

**Tech Stack:** Rust, async-trait, schemars, thiserror, tokio

## Global Constraints

- Workspace uses `version.workspace = true` and `edition.workspace = true` in all crate Cargo.toml files
- Workspace deps are declared in root `Cargo.toml` `[workspace.dependencies]` with `path = "crates/wisp-*"`
- All crates use `wisp-` prefix
- `serde_json::Value` is used for JSON schema fields (not `schemars::schema::SchemaObject`)
- `thiserror` workspace version is `2.0.12`
- `async-trait` workspace version is `0.1`
- No comments in code unless explicitly part of a doc string that already exists in the codebase

---

## File Structure

### New files

| Path | Responsibility |
|---|---|
| `crates/wisp-tool-registry/Cargo.toml` | Crate manifest |
| `crates/wisp-tool-registry/src/lib.rs` | Module declarations + re-exports |
| `crates/wisp-tool-registry/src/types.rs` | `ToolDefinition`, `ToolAnnotations`, `registered_name()` |
| `crates/wisp-tool-registry/src/handler.rs` | `ToolHandler` trait |
| `crates/wisp-tool-registry/src/registry.rs` | `ToolRegistry` with `Arc<dyn ToolHandler>` dispatch |
| `crates/wisp-software-tools/Cargo.toml` | Crate manifest |
| `crates/wisp-software-tools/src/lib.rs` | Module declarations + re-exports |
| `crates/wisp-software-tools/src/trait.rs` | `NativeTool` trait |
| `crates/wisp-software-tools/src/registry.rs` | `SoftwareToolRegistry` |
| `crates/wisp-software-tools/src/error.rs` | `NativeToolError` |

### Modified files

| Path | Change |
|---|---|
| `Cargo.toml` (root) | Add `wisp-tool-registry` and `wisp-software-tools` to workspace deps |
| `crates/wisp-common/src/lib.rs` | Re-export new `tool_types` module |
| `crates/wisp-common/src/tool_types.rs` | `ToolContent`, `ToolResult`, `ToolError` (moved from wisp-mcp) |
| `crates/wisp-common/Cargo.toml` | No new deps needed |
| `crates/wisp-mcp/src/lib.rs` | Remove `tool_registry` module, re-export from new crates |
| `crates/wisp-mcp/src/mcp_handler.rs` | `McpToolHandler` impl of `ToolHandler` |
| `crates/wisp-mcp/Cargo.toml` | Add `wisp-tool-registry` dep |
| `crates/wisp-mcp/src/tool_registry/*` | **Deleted** |
| `src-tauri/Cargo.toml` | Add `wisp-tool-registry`, `wisp-software-tools` deps |
| `src-tauri/src/lib.rs` | Update setup to create `SoftwareToolRegistry`, register native tools |
| `src-tauri/src/types.rs` | Update `AppData` imports |
| `src-tauri/src/registry_commands.rs` | Update imports, use `register_mcp_tools()` |
| `src-tauri/src/conversation_commands.rs` | Update imports, pass `pal_id: None` to `execute()` |

---

## Task 1: Move shared types to `wisp-common`

**Files:**
- Create: `crates/wisp-common/src/tool_types.rs`
- Modify: `crates/wisp-common/src/lib.rs`

**Interfaces:**
- Produces: `wisp_common::ToolContent`, `wisp_common::ToolResult`, `wisp_common::ToolError`, `wisp_common::ToolResultExt` (the `from_mcp_response` method)

- [ ] **Step 1: Create `crates/wisp-common/src/tool_types.rs`**

Move `ToolContent`, `ToolResult`, and `ToolError` from `crates/wisp-mcp/src/tool_registry/types.rs` (lines 49-99, 103-174). These are the shared result types with no MCP-specific dependencies:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    #[serde(default)]
    pub content: Vec<ToolContent>,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ToolContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    #[serde(rename = "resource")]
    Resource {
        uri: String,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "mimeType")]
        mime_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blob: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolError {
    NotFound(String),
    ExecutionFailed(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::NotFound(name) => write!(f, "tool not found: {name}"),
            ToolError::ExecutionFailed(reason) => write!(f, "tool execution failed: {reason}"),
        }
    }
}

impl std::error::Error for ToolError {}

impl ToolResult {
    pub fn from_mcp_response(raw: serde_json::Value) -> Self {
        let is_error = raw
            .get("isError")
            .or_else(|| raw.get("is_error"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let content = raw
            .get("content")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|item| {
                        let type_str = item
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("text");
                        match type_str {
                            "text" => ToolContent::Text {
                                text: item
                                    .get("text")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                            },
                            "image" => ToolContent::Image {
                                data: item
                                    .get("data")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                mime_type: item
                                    .get("mimeType")
                                    .or_else(|| item.get("mime_type"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("image/png")
                                    .to_string(),
                            },
                            "resource" => ToolContent::Resource {
                                uri: item
                                    .get("uri")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                mime_type: item
                                    .get("mimeType")
                                    .or_else(|| item.get("mime_type"))
                                    .and_then(|v| v.as_str())
                                    .map(String::from),
                                text: item
                                    .get("text")
                                    .and_then(|v| v.as_str())
                                    .map(String::from),
                                blob: item
                                    .get("blob")
                                    .and_then(|v| v.as_str())
                                    .map(String::from),
                            },
                            other => ToolContent::Text {
                                text: format!("[Unsupported content type: {other}]"),
                            },
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        ToolResult { content, is_error }
    }
}
```

Copy the tests for `from_mcp_response` from the original file (lines 254-380) into a `#[cfg(test)] mod tests` block at the bottom of this new file.

- [ ] **Step 2: Update `crates/wisp-common/src/lib.rs`**

```rust
pub mod tool_types;
pub mod types;
pub mod utils;

pub use tool_types::{ToolContent, ToolError, ToolResult};
pub use types::{MessageSource, McpConnectionStatusEvent};
pub use utils::{compute_content_hash, get_uuid_v4};
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p wisp-common`
Expected: PASS (no errors)

- [ ] **Step 4: Run tests**

Run: `cargo test -p wisp-common`
Expected: All tests pass (including the moved `from_mcp_response` tests)

- [ ] **Step 5: Commit**

```bash
git add crates/wisp-common/src/tool_types.rs crates/wisp-common/src/lib.rs
git commit -m "refactor: move ToolContent/ToolResult/ToolError to wisp-common"
```

---

## Task 2: Create `wisp-tool-registry` crate

**Files:**
- Create: `crates/wisp-tool-registry/Cargo.toml`
- Create: `crates/wisp-tool-registry/src/lib.rs`
- Create: `crates/wisp-tool-registry/src/types.rs`
- Create: `crates/wisp-tool-registry/src/handler.rs`
- Create: `crates/wisp-tool-registry/src/registry.rs`

**Interfaces:**
- Consumes: `wisp_common::ToolContent`, `wisp_common::ToolResult`, `wisp_common::ToolError`
- Produces: `ToolDefinition`, `ToolAnnotations`, `registered_name()`, `ToolHandler` trait, `ToolRegistry`

- [ ] **Step 1: Create `crates/wisp-tool-registry/Cargo.toml`**

```toml
[package]
name = "wisp-tool-registry"
version.workspace = true
edition.workspace = true

[dependencies]
wisp-common.workspace = true
serde.workspace = true
serde_json.workspace = true
async-trait.workspace = true
thiserror.workspace = true
tokio = { workspace = true, features = ["sync"] }
```

- [ ] **Step 2: Create `crates/wisp-tool-registry/src/types.rs`**

Move `ToolDefinition`, `ToolAnnotations`, and `registered_name()` from `crates/wisp-mcp/src/tool_registry/types.rs`. Remove the `enabled` field from `ToolDefinition` and add `requires_confirmation`:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: Option<String>,
    #[serde(default = "default_input_schema")]
    pub input_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub requires_confirmation: bool,
}

fn default_input_schema() -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": {} })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolAnnotations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

pub fn registered_name(server_id: &str, tool_name: &str) -> String {
    let clean = |s: &str| -> String {
        let mut out = String::new();
        let mut prev_underscore = false;
        for ch in s.chars() {
            let mapped = if ch.is_ascii_alphanumeric() || ch == '_' { ch } else { '_' };
            if mapped == '_' {
                if !prev_underscore {
                    out.push('_');
                    prev_underscore = true;
                }
            } else {
                out.push(mapped.to_ascii_lowercase());
                prev_underscore = false;
            }
        }
        out.trim_matches('_').to_string()
    };

    let left = clean(server_id);
    let right = clean(tool_name);
    match (left.is_empty(), right.is_empty()) {
        (true, true) => "mcp_".to_string(),
        (false, true) => format!("mcp_{left}"),
        (true, false) => format!("mcp_{right}"),
        (false, false) => format!("mcp_{left}_{right}"),
    }
}
```

Copy the `registered_name` tests from the original file (lines 224-252).

- [ ] **Step 3: Create `crates/wisp-tool-registry/src/handler.rs`**

```rust
use async_trait::async_trait;
use serde_json::Value;
use wisp_common::{ToolError, ToolResult};

#[async_trait]
pub trait ToolHandler: Send + Sync {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError>;
}
```

- [ ] **Step 4: Create `crates/wisp-tool-registry/src/registry.rs`**

This is the refactored `ToolRegistry`. Key changes from the original:
- No `McpStdioManager` / `McpHttpManager` fields
- `ToolEntry` holds `Arc<dyn ToolHandler>` instead of transport info
- `Inner` gains `allowed_pals: HashMap<String, Vec<String>>`
- `register()` takes a `ToolDefinition` + `Arc<dyn ToolHandler>` + `allowed_pals`
- `execute()` gains `pal_id: Option<&str>` parameter
- `set_server_enabled()` removed (server grouping no longer tracked — use `unregister_by_metadata` or iterate)

```rust
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
        let entry = {
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

        entry.handler_ref().execute(args).await
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
```

Wait — `entry.handler_ref()` doesn't exist. I need to return the `Arc<dyn ToolHandler>` from the locked section. Let me fix this. The `execute` method should clone the `Arc` out of the lock, then call `.execute()` on it. Fix:

Replace the end of `execute()`:

```rust
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
```

Write the tests. Adapt the original tests from `registry.rs` — they need mock `ToolHandler` implementations instead of real MCP managers:

```rust
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
```

- [ ] **Step 5: Create `crates/wisp-tool-registry/src/lib.rs`**

```rust
pub mod handler;
pub mod registry;
pub mod types;

pub use handler::ToolHandler;
pub use registry::ToolRegistry;
pub use types::{registered_name, ToolAnnotations, ToolDefinition};
```

- [ ] **Step 6: Add to root `Cargo.toml`**

In the `[workspace.dependencies]` section, after the `wisp-mcp` line:

```toml
wisp-tool-registry = { path = "crates/wisp-tool-registry" }
```

- [ ] **Step 7: Verify it compiles**

Run: `cargo check -p wisp-tool-registry`
Expected: PASS

- [ ] **Step 8: Run tests**

Run: `cargo test -p wisp-tool-registry`
Expected: All tests pass

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml crates/wisp-tool-registry/
git commit -m "feat: add wisp-tool-registry crate with ToolHandler trait"
```

---

## Task 3: Refactor `wisp-mcp` to use `wisp-tool-registry`

**Files:**
- Create: `crates/wisp-mcp/src/mcp_handler.rs`
- Modify: `crates/wisp-mcp/src/lib.rs`
- Modify: `crates/wisp-mcp/Cargo.toml`
- Delete: `crates/wisp-mcp/src/tool_registry/` (entire directory)

**Interfaces:**
- Consumes: `wisp_tool_registry::{ToolHandler, ToolDefinition, ToolAnnotations, registered_name}`, `wisp_common::{ToolContent, ToolResult, ToolError}`
- Produces: `McpToolHandler`, `register_mcp_tools()`

- [ ] **Step 1: Update `crates/wisp-mcp/Cargo.toml`**

Add `wisp-tool-registry` and `async-trait` dependencies:

```toml
[package]
name = "wisp-mcp"
version.workspace = true
edition.workspace = true

[dependencies]
wisp-common.workspace = true
wisp-tool-registry.workspace = true
serde.workspace = true
serde_json.workspace = true
tauri.workspace = true
tokio.workspace = true
reqwest.workspace = true
futures.workspace = true
anyhow.workspace = true
async-trait.workspace = true
```

- [ ] **Step 2: Create `crates/wisp-mcp/src/mcp_handler.rs`**

```rust
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wisp_common::{ToolError, ToolResult};
use wisp_tool_registry::{registered_name, ToolAnnotations, ToolDefinition, ToolHandler, ToolRegistry};

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
        McpToolHandler {
            server_id,
            original_name,
            stdio_manager,
            http_manager,
            transport,
        }
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
                .map_err(|e| ToolError::ExecutionFailed(format!(
                    "MCP stdio, server '{}': {e}", self.server_id
                )))?,
            TransportConfig::Sse { .. } | TransportConfig::Http { .. } => self
                .http_manager
                .call_tool(&self.server_id, &self.original_name, mcp_args)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!(
                    "MCP http, server '{}': {e}", self.server_id
                )))?,
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
```

- [ ] **Step 3: Update `crates/wisp-mcp/src/lib.rs`**

Remove `tool_registry` module. Re-export from `wisp-tool-registry` and `wisp-common` for backward compatibility:

```rust
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
```

- [ ] **Step 4: Delete the old `tool_registry` directory**

```bash
rm -rf crates/wisp-mcp/src/tool_registry/
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p wisp-mcp`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add -A crates/wisp-mcp/
git commit -m "refactor: wisp-mcp uses wisp-tool-registry, McpToolHandler implements ToolHandler"
```

---

## Task 4: Update `src-tauri` to use the new crate structure

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/types.rs`
- Modify: `src-tauri/src/registry_commands.rs`
- Modify: `src-tauri/src/conversation_commands.rs`

**Interfaces:**
- Consumes: `wisp_tool_registry::ToolRegistry`, `wisp_mcp::register_mcp_tools`
- Produces: Updated `AppData` with `ToolRegistry::new()` (no manager args), `registry_commands` using `register_mcp_tools`

- [ ] **Step 1: Update `src-tauri/Cargo.toml`**

Add the new crate deps:

```toml
[dependencies]
wisp-common.workspace = true
wisp-keyring.workspace = true
wisp-db.workspace = true
wisp-configs.workspace = true
wisp-llm.workspace = true
wisp-mcp.workspace = true
wisp-tool-registry.workspace = true
wisp-conversation.workspace = true
```

- [ ] **Step 2: Update `src-tauri/src/types.rs`**

Change the import — `ToolRegistry` now comes from `wisp_tool_registry`:

```rust
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
    pub config_manager: ConfigManager,
    pub mcp_config_manager: McpConfigManager,
    pub mcp_stdio_manager: Arc<McpStdioManager>,
    pub mcp_http_manager: Arc<McpHttpManager>,
    pub tool_registry: Arc<ToolRegistry>,
    pub unlocked_pals: HashMap<String, HashSet<String>>,
}
```

- [ ] **Step 3: Update `src-tauri/src/lib.rs`**

Change `ToolRegistry::new()` — it no longer takes manager args:

Replace lines 56-59:
```rust
			        let tool_registry = std::sync::Arc::new(ToolRegistry::new());
```

Update the import on line 27:
```rust
use wisp_tool_registry::ToolRegistry;
```

Remove the now-unused import of `ToolRegistry` from `wisp_mcp` (line 27 originally). Keep the `TransportConfig` import from `wisp_mcp` (line 24).

- [ ] **Step 4: Update `src-tauri/src/registry_commands.rs`**

Change imports and the `register_server` call to use `register_mcp_tools`:

```rust
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use crate::types::AppData;
use wisp_mcp::{NormalizedTool, TransportConfig, register_mcp_tools};
use wisp_tool_registry::ToolDefinition;
use wisp_common::ToolResult;
```

In `registry_refresh`, replace the `state.tool_registry.register_server(...)` call (line 84) with:

```rust
            register_mcp_tools(
                &state.tool_registry,
                server_id,
                tools,
                transport,
                std::sync::Arc::clone(&state.mcp_stdio_manager),
                std::sync::Arc::clone(&state.mcp_http_manager),
            );
```

The `register_mcp_tools` function needs the managers. They're available from `state`. Update the lock block to also clone the manager arcs before the loop. The full updated section (lines 80-87):

```rust
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
```

In `registry_list_tools`, the `enabled` field no longer exists on `ToolDefinition`. Remove lines 97-101 (the enabled sync loop). The function becomes:

```rust
pub async fn registry_list_tools(app_handle: AppHandle) -> Result<Vec<ToolDefinition>, String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    let mut tools: Vec<ToolDefinition> = state.tool_registry.list_tools();
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(tools)
}
```

In `registry_execute`, add `pal_id: None` to the `execute()` call (line 120):

```rust
    registry
        .execute(&name, args, None)
        .await
        .map_err(|e| e.to_string())
```

- [ ] **Step 5: Update `src-tauri/src/conversation_commands.rs`**

Change the import on line 21 from `wisp_mcp` to `wisp_common`:

```rust
use wisp_common::{ToolContent, MessageSource};
```

Remove the old `use wisp_mcp::{ToolContent, ToolDefinition};` line.

Add `use wisp_tool_registry::ToolDefinition;` if `ToolDefinition` is used elsewhere in the file (check with: `grep ToolDefinition src-tauri/src/conversation_commands.rs`).

In `execute_tool_call`, add `pal_id: None` to the execute call (line 134):

```rust
    let result = registry
        .execute(&call.name, call.arguments.clone(), None)
        .await
        .map_err(|error| format!("Tool '{}' failed: {}", call.name, error))?;
```

- [ ] **Step 6: Verify the whole project compiles**

Run: `cargo check`
Expected: PASS (zero errors across all crates)

- [ ] **Step 7: Run all existing tests**

Run: `cargo test`
Expected: All tests pass (the moved tests now live in wisp-common and wisp-tool-registry)

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor: src-tauri uses wisp-tool-registry, ToolRegistry::new() takes no args"
```

---

## Task 5: Create `wisp-software-tools` crate

**Files:**
- Create: `crates/wisp-software-tools/Cargo.toml`
- Create: `crates/wisp-software-tools/src/lib.rs`
- Create: `crates/wisp-software-tools/src/error.rs`
- Create: `crates/wisp-software-tools/src/trait.rs`
- Create: `crates/wisp-software-tools/src/registry.rs`

**Interfaces:**
- Consumes: `wisp_tool_registry::{ToolHandler, ToolDefinition, ToolRegistry, ToolAnnotations}`, `wisp_common::{ToolContent, ToolResult, ToolError}`
- Produces: `NativeTool` trait, `SoftwareToolRegistry`, `NativeToolError`

- [ ] **Step 1: Create `crates/wisp-software-tools/Cargo.toml`**

```toml
[package]
name = "wisp-software-tools"
version.workspace = true
edition.workspace = true

[dependencies]
wisp-common.workspace = true
wisp-tool-registry.workspace = true
serde.workspace = true
serde_json.workspace = true
schemars = "2"
async-trait.workspace = true
thiserror.workspace = true
```

- [ ] **Step 2: Create `crates/wisp-software-tools/src/error.rs`**

```rust
#[derive(Debug, thiserror::Error)]
pub enum NativeToolError {
    #[error("argument validation failed: {0}")]
    Validation(String),
    #[error("execution error: {0}")]
    Runtime(String),
    #[error("permission denied: {0}")]
    Permission(String),
}

impl From<NativeToolError> for wisp_common::ToolError {
    fn from(e: NativeToolError) -> Self {
        wisp_common::ToolError::ExecutionFailed(e.to_string())
    }
}
```

- [ ] **Step 3: Create `crates/wisp-software-tools/src/trait.rs`**

```rust
use async_trait::async_trait;
use serde_json::Value;
use wisp_tool_registry::ToolHandler;

#[async_trait]
pub trait NativeTool: ToolHandler {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> Value;
    fn requires_confirmation(&self) -> bool {
        false
    }
    fn default_allowed_pals(&self) -> Vec<String> {
        vec![]
    }
}
```

- [ ] **Step 4: Create `crates/wisp-software-tools/src/registry.rs`**

Write the test first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde::Deserialize;
    use schemars::JsonSchema;
    use wisp_common::{ToolContent, ToolError, ToolResult};
    use wisp_tool_registry::{ToolDefinition, ToolHandler, ToolRegistry};

    struct EchoTool;

    #[derive(Deserialize, JsonSchema)]
    struct EchoArgs {
        msg: String,
    }

    #[async_trait]
    impl ToolHandler for EchoTool {
        async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError> {
            let args: EchoArgs = serde_json::from_value(args)
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
            Ok(ToolResult {
                content: vec![ToolContent::Text { text: args.msg }],
                is_error: false,
            })
        }
    }

    impl NativeTool for EchoTool {
        fn name(&self) -> &str { "test_echo" }
        fn description(&self) -> &str { "Echo back the message" }
        fn schema(&self) -> serde_json::Value {
            schemars::schema_for!(EchoArgs)
        }
    }

    #[test]
    fn test_register_and_list_definitions() {
        let mut sw = SoftwareToolRegistry::new();
        sw.register(EchoTool);
        let defs = sw.list_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "test_echo");
        assert!(!defs[0].requires_confirmation);
    }

    #[test]
    fn test_register_into_populates_registry() {
        let mut sw = SoftwareToolRegistry::new();
        sw.register(EchoTool);
        let registry = ToolRegistry::new();
        sw.register_into(&registry);
        assert!(registry.get_tool("test_echo").is_some());
        assert!(registry.enabled_set().contains("test_echo"));
    }

    #[tokio::test]
    async fn test_execute_via_registry() {
        let mut sw = SoftwareToolRegistry::new();
        sw.register(EchoTool);
        let registry = ToolRegistry::new();
        sw.register_into(&registry);
        let result = registry
            .execute("test_echo", serde_json::json!({"msg": "hello"}), None)
            .await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(!result.is_error);
        assert_eq!(
            result.content[0],
            ToolContent::Text { text: "hello".to_string() }
        );
    }

    #[test]
    fn test_definition_has_schema() {
        let mut sw = SoftwareToolRegistry::new();
        sw.register(EchoTool);
        let defs = sw.list_definitions();
        let schema = &defs[0].input_schema;
        assert!(schema.get("properties").is_some());
        assert!(schema["properties"].get("msg").is_some());
    }
}
```

Now the implementation:

```rust
use std::collections::HashMap;
use std::sync::Arc;

use wisp_tool_registry::{ToolDefinition, ToolHandler, ToolRegistry};

use super::trait_def::NativeTool;

pub struct SoftwareToolRegistry {
    tools: HashMap<String, Arc<dyn NativeTool>>,
}

impl SoftwareToolRegistry {
    pub fn new() -> Self {
        SoftwareToolRegistry {
            tools: HashMap::new(),
        }
    }

    pub fn register<T: NativeTool + 'static>(&mut self, tool: T) -> &mut Self {
        let name = tool.name().to_string();
        self.tools.insert(name, Arc::new(tool));
        self
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn NativeTool>> {
        self.tools.get(name).cloned()
    }

    pub fn list_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|t| ToolDefinition {
                name: t.name().to_string(),
                description: Some(t.description().to_string()),
                input_schema: t.schema(),
                annotations: None,
                metadata: std::collections::HashMap::from([
                    ("provider".to_string(), serde_json::Value::String("native".to_string())),
                ]),
                requires_confirmation: t.requires_confirmation(),
            })
            .collect()
    }

    pub fn register_into(&self, registry: &ToolRegistry) {
        for tool in self.tools.values() {
            let name = tool.name().to_string();
            let definition = ToolDefinition {
                name: name.clone(),
                description: Some(tool.description().to_string()),
                input_schema: tool.schema(),
                annotations: None,
                metadata: std::collections::HashMap::from([
                    ("provider".to_string(), serde_json::Value::String("native".to_string())),
                ]),
                requires_confirmation: tool.requires_confirmation(),
            };
            let handler: Arc<dyn ToolHandler> = Arc::clone(tool) as Arc<dyn ToolHandler>;
            registry.register(definition, handler, tool.default_allowed_pals());
        }
    }
}

impl Default for SoftwareToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

Wait — `Arc<dyn NativeTool>` cannot be directly cast to `Arc<dyn ToolHandler>` because `NativeTool: ToolHandler` is a supertrait, but `Arc<dyn NativeTool>` and `Arc<dyn ToolHandler>` are different fat-pointer types. We need a wrapper or use the concrete type.

Actually, since `NativeTool: ToolHandler`, any type implementing `NativeTool` also implements `ToolHandler`. But `Arc<dyn NativeTool>` is a trait object for `NativeTool`, and we can't automatically convert it to `Arc<dyn ToolHandler>`.

The solution: store `Arc<dyn NativeTool>` and create a wrapper that implements `ToolHandler` by delegating. Or, simpler: store the tools as concrete types using a different approach.

Actually the simplest fix: the `SoftwareToolRegistry` should store both the `Arc<dyn NativeTool>` (for metadata access) and when registering into `ToolRegistry`, we need a `ToolHandler`. Since `NativeTool: ToolHandler`, we can use the concrete tool type if we store it generically.

Better approach: Don't store `Arc<dyn NativeTool>`. Instead, have `register()` capture everything we need upfront (definition + handler) and store those separately:

Actually, let me rethink. The cleanest way: store tools as `Box<dyn NativeTool>` but when registering into `ToolRegistry`, we need `Arc<dyn ToolHandler>`. We can't easily upcast.

The pragmatic solution is a wrapper struct:

```rust
struct NativeToolWrapper {
    tool: Arc<dyn NativeTool>,
}

#[async_trait]
impl ToolHandler for NativeToolWrapper {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        self.tool.execute(args).await
    }
}
```

Wait, that won't work either because `self.tool.execute()` calls `ToolHandler::execute()` on `dyn NativeTool`, but we can't call supertrait methods on trait objects in older Rust. Actually, in recent Rust (1.86+), trait object upcasting is stable. Let me check — trait upcasting stabilization was in Rust 1.86.0 (April 2025). If the project uses a recent Rust version, this works.

Alternatively, add `fn execute()` to the `NativeTool` trait itself as a required method (redundant with `ToolHandler` but avoids upcasting). Or use the wrapper approach.

Safest approach for compatibility: the `NativeTool` trait defines `run()` instead of relying on `ToolHandler::execute()`, and a blanket impl bridges them:

Actually, let me simplify. The cleanest design that definitely works:

```rust
// trait.rs
#[async_trait]
pub trait NativeTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> Value;
    fn requires_confirmation(&self) -> bool { false }
    fn default_allowed_pals(&self) -> Vec<String> { vec![] }
    async fn run(&self, args: Value) -> Result<ToolResult, ToolError>;
}
```

And `NativeTool` does NOT extend `ToolHandler`. Instead, `SoftwareToolRegistry::register_into()` creates a closure-based handler or wrapper:

```rust
struct NativeToolAdapter {
    tool: Arc<dyn NativeTool>,
}

#[async_trait]
impl ToolHandler for NativeToolAdapter {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        self.tool.run(args).await
    }
}
```

This is clean and always works. `NativeTool` has `run()`, the adapter bridges to `ToolHandler::execute()`.

Let me rewrite the trait and registry with this approach.

- [ ] **Step 4 (revised): Create `crates/wisp-software-tools/src/trait.rs`**

```rust
use async_trait::async_trait;
use serde_json::Value;
use wisp_common::{ToolError, ToolResult};

#[async_trait]
pub trait NativeTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> Value;
    fn requires_confirmation(&self) -> bool {
        false
    }
    fn default_allowed_pals(&self) -> Vec<String> {
        vec![]
    }
    async fn run(&self, args: Value) -> Result<ToolResult, ToolError>;
}
```

- [ ] **Step 5 (revised): Create `crates/wisp-software-tools/src/registry.rs`**

Write the full file with adapter:

```rust
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wisp_common::{ToolError, ToolResult};
use wisp_tool_registry::{ToolDefinition, ToolHandler, ToolRegistry};

use crate::trait_def::NativeTool;

struct NativeToolAdapter {
    tool: Arc<dyn NativeTool>,
}

#[async_trait]
impl ToolHandler for NativeToolAdapter {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        self.tool.run(args).await
    }
}

pub struct SoftwareToolRegistry {
    tools: HashMap<String, Arc<dyn NativeTool>>,
}

impl SoftwareToolRegistry {
    pub fn new() -> Self {
        SoftwareToolRegistry {
            tools: HashMap::new(),
        }
    }

    pub fn register<T: NativeTool + 'static>(&mut self, tool: T) -> &mut Self {
        let name = tool.name().to_string();
        self.tools.insert(name, Arc::new(tool));
        self
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn NativeTool>> {
        self.tools.get(name).cloned()
    }

    pub fn list_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|t| self.build_definition(t))
            .collect()
    }

    pub fn register_into(&self, registry: &ToolRegistry) {
        for tool in self.tools.values() {
            let definition = self.build_definition(tool);
            let handler = Arc::new(NativeToolAdapter { tool: Arc::clone(tool) });
            registry.register(definition, handler, tool.default_allowed_pals());
        }
    }

    fn build_definition(&self, tool: &Arc<dyn NativeTool>) -> ToolDefinition {
        ToolDefinition {
            name: tool.name().to_string(),
            description: Some(tool.description().to_string()),
            input_schema: tool.schema(),
            annotations: None,
            metadata: std::collections::HashMap::from([
                ("provider".to_string(), Value::String("native".to_string())),
            ]),
            requires_confirmation: tool.requires_confirmation(),
        }
    }
}

impl Default for SoftwareToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

Update the tests to use `run()` instead of `execute()`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use schemars::JsonSchema;

    struct EchoTool;

    #[derive(Deserialize, JsonSchema)]
    struct EchoArgs {
        msg: String,
    }

    #[async_trait]
    impl NativeTool for EchoTool {
        fn name(&self) -> &str { "test_echo" }
        fn description(&self) -> &str { "Echo back the message" }
        fn schema(&self) -> serde_json::Value {
            schemars::schema_for!(EchoArgs)
        }
        async fn run(&self, args: Value) -> Result<ToolResult, ToolError> {
            let args: EchoArgs = serde_json::from_value(args)
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
            Ok(ToolResult {
                content: vec![wisp_common::ToolContent::Text { text: args.msg }],
                is_error: false,
            })
        }
    }

    #[test]
    fn test_register_and_list_definitions() {
        let mut sw = SoftwareToolRegistry::new();
        sw.register(EchoTool);
        let defs = sw.list_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "test_echo");
        assert!(!defs[0].requires_confirmation);
    }

    #[test]
    fn test_register_into_populates_registry() {
        let mut sw = SoftwareToolRegistry::new();
        sw.register(EchoTool);
        let registry = ToolRegistry::new();
        sw.register_into(&registry);
        assert!(registry.get_tool("test_echo").is_some());
        assert!(registry.enabled_set().contains("test_echo"));
    }

    #[tokio::test]
    async fn test_execute_via_registry() {
        let mut sw = SoftwareToolRegistry::new();
        sw.register(EchoTool);
        let registry = ToolRegistry::new();
        sw.register_into(&registry);
        let result = registry
            .execute("test_echo", serde_json::json!({"msg": "hello"}), None)
            .await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(!result.is_error);
        assert_eq!(
            result.content[0],
            wisp_common::ToolContent::Text { text: "hello".to_string() }
        );
    }

    #[test]
    fn test_definition_has_schema() {
        let mut sw = SoftwareToolRegistry::new();
        sw.register(EchoTool);
        let defs = sw.list_definitions();
        let schema = &defs[0].input_schema;
        assert!(schema.get("properties").is_some());
        assert!(schema["properties"].get("msg").is_some());
    }
}
```

- [ ] **Step 6: Create `crates/wisp-software-tools/src/lib.rs`**

```rust
pub mod error;
pub mod registry;
pub mod trait_def;

pub use error::NativeToolError;
pub use registry::SoftwareToolRegistry;
pub use trait_def::NativeTool;
```

Note: the module is `trait_def` (not `trait`) because `trait` is a reserved keyword.

- [ ] **Step 7: Add to root `Cargo.toml`**

In the `[workspace.dependencies]` section:

```toml
wisp-software-tools = { path = "crates/wisp-software-tools" }
```

- [ ] **Step 8: Verify it compiles**

Run: `cargo check -p wisp-software-tools`
Expected: PASS

- [ ] **Step 9: Run tests**

Run: `cargo test -p wisp-software-tools`
Expected: All 4 tests pass

- [ ] **Step 10: Commit**

```bash
git add Cargo.toml crates/wisp-software-tools/
git commit -m "feat: add wisp-software-tools crate with NativeTool trait and SoftwareToolRegistry"
```

---

## Task 6: Wire `SoftwareToolRegistry` into app setup

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `wisp_software_tools::SoftwareToolRegistry`
- Produces: App startup creates a `SoftwareToolRegistry`, registers it into `ToolRegistry`

- [ ] **Step 1: Update `src-tauri/Cargo.toml`**

Add `wisp-software-tools`:

```toml
wisp-software-tools.workspace = true
```

- [ ] **Step 2: Update `src-tauri/src/lib.rs`**

Add import:
```rust
use wisp_software_tools::SoftwareToolRegistry;
```

After the `tool_registry` creation (line ~56), add:

```rust
			        let software_registry = SoftwareToolRegistry::new();
			        software_registry.register_into(&tool_registry);
```

Currently `SoftwareToolRegistry::new()` returns a value and `register_into` takes `&ToolRegistry`. But `tool_registry` is behind `Arc`. Since `register_into` takes `&ToolRegistry` and `Arc<ToolRegistry>` derefs to `&ToolRegistry`, this works with `&tool_registry` or `tool_registry.as_ref()`.

Wait — `tool_registry` is `Arc<ToolRegistry>` created on line 56. `register_into` needs `&ToolRegistry`. `Arc<T>` implements `Deref<Target=T>`, so `&*tool_registry` or just passing `&tool_registry` with auto-deref works. Let me use explicit deref for clarity:

```rust
			        let tool_registry = std::sync::Arc::new(ToolRegistry::new());

			        {
			            let software_registry = SoftwareToolRegistry::new();
			            software_registry.register_into(&tool_registry);
			        }
```

Since `register_into` is `&self`, and we're borrowing `tool_registry` (which is `Arc<ToolRegistry>`), Rust auto-derefs `&Arc<ToolRegistry>` to `&ToolRegistry`. This works.

For now, no native tools are registered (the `SoftwareToolRegistry` is empty). This is intentional — it verifies the wiring compiles and the app boots. Concrete tools will be added in a follow-up.

- [ ] **Step 3: Verify the whole project compiles**

Run: `cargo check`
Expected: PASS

- [ ] **Step 4: Run all tests**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/lib.rs
git commit -m "feat: wire SoftwareToolRegistry into app setup"
```

---

## Self-Review Notes

**Spec coverage:**
- ToolContent/ToolResult/ToolError → wisp-common: Task 1 ✓
- wisp-tool-registry crate with ToolDefinition (requires_confirmation, no enabled field): Task 2 ✓
- ToolHandler trait: Task 2 ✓
- ToolRegistry with Arc<dyn ToolHandler> + pal_id + allowed_pals: Task 2 ✓
- McpToolHandler + register_mcp_tools: Task 3 ✓
- src-tauri migration: Task 4 ✓
- wisp-software-tools with NativeTool trait + SoftwareToolRegistry: Task 5 ✓
- App wiring: Task 6 ✓
- Frontend changes: **Deferred to follow-up plan** (noted in spec)
- Confirmation gates: **Deferred to follow-up plan** (requires frontend)
- Built-in tools (settings get/set, js_exec): **Deferred to follow-up plan**

**Placeholder scan:** No TBD/TODO. All code is complete.

**Type consistency:**
- `execute()` signature: `(name, args, pal_id: Option<&str>)` — consistent across Task 2, 4, and 5
- `NativeTool::run()` — consistent in Task 5 trait + tests
- `register()` takes `(ToolDefinition, Arc<dyn ToolHandler>, Vec<String>)` — consistent in Task 2 and Task 5

# Software Tools — Native Tool Framework Design

## Problem

Wisp currently routes all tool calls through MCP (stdio/http). Tools that belong to the app itself — changing settings, executing JavaScript, querying internal state — either require a local MCP server or don't exist. Additionally, `ToolRegistry` lives inside `wisp-mcp` despite being a general dispatch layer that will now serve both MCP and native tools.

This design adds a native tool framework and extracts `ToolRegistry` into its own crate so the dependency graph reflects reality: MCP is one tool *provider*, not the owner of tool dispatch.

## Crate layout

```
wisp-common            ToolContent, ToolResult, ToolError (shared result types)
wisp-tool-registry     ToolDefinition, ToolAnnotations, ToolRegistry, ToolHandler trait, permission state
wisp-mcp               MCP protocol (stdio/http clients, managers) — implements ToolHandler
wisp-software-tools    NativeTool trait (extends ToolHandler), SoftwareToolRegistry, built-in tools
```

Dependency graph:

```
wisp-common              (standalone)
wisp-tool-registry       → wisp-common
wisp-mcp                 → wisp-tool-registry, wisp-common
wisp-software-tools      → wisp-tool-registry, wisp-common
wisp-conversation        → wisp-tool-registry (for ToolDefinition), wisp-common
src-tauri                → all
```

## Design

### `wisp-common` changes

Move `ToolContent`, `ToolResult`, and `ToolError` from `wisp-mcp` into `wisp-common`. These are the shared result types used across the entire pipeline. `wisp-mcp` re-exports them for backward compatibility during migration.

### Crate: `wisp-tool-registry` (new — extracted from `wisp-mcp`)

Holds everything related to tool definition, registration, dispatch, and permissions. No MCP-specific knowledge.

#### ToolHandler trait

The execution seam. Both MCP and native tools conform to it:

```rust
#[async_trait]
pub trait ToolHandler: Send + Sync {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError>;
}
```

#### ToolDefinition

```rust
pub struct ToolDefinition {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
    pub annotations: Option<ToolAnnotations>,
    pub metadata: HashMap<String, Value>,
    pub requires_confirmation: bool,   // intrinsic — set by tool author
}
```

`enabled` is **not** on `ToolDefinition`. It lives in the registry's runtime state.

#### ToolRegistry

```rust
pub struct ToolRegistry {
    inner: Mutex<Inner>,
}

struct Inner {
    entries: HashMap<String, ToolEntry>,
    enabled: HashSet<String>,
    allowed_pals: HashMap<String, Vec<String>>,   // tool_name -> allowed pal IDs
}

struct ToolEntry {
    definition: ToolDefinition,
    handler: Arc<dyn ToolHandler>,
}
```

Key methods:

```rust
impl ToolRegistry {
    pub fn new() -> Self;

    /// Register a tool with its definition and execution handler.
    /// Used by both MCP layer (wrapping manager calls) and software tools.
    pub fn register(
        &self,
        definition: ToolDefinition,
        handler: Arc<dyn ToolHandler>,
        allowed_pals: Vec<String>,      // empty = unrestricted
    );

    pub fn unregister(&self, name: &str);
    pub fn get_tool(&self, name: &str) -> Option<ToolDefinition>;
    pub fn list_tools(&self) -> Vec<ToolDefinition>;
    pub fn list_enabled_tools(&self) -> Vec<ToolDefinition>;

    pub fn set_tool_enabled(&self, name: &str, enabled: bool);
    pub fn set_tool_allowed_pals(&self, name: &str, pal_ids: Vec<String>);

    /// Pal-aware execution. Checks enabled + allowed_pals before dispatching.
    pub async fn execute(
        &self,
        name: &str,
        args: Value,
        pal_id: Option<&str>,
    ) -> Result<ToolResult, ToolError>;
}
```

#### execute() flow

1. Look up the tool entry. If not found, return `ToolError::NotFound`.
2. Check `enabled` — reject if disabled.
3. **Pal check** — if `allowed_pals` is non-empty and `pal_id` is `Some`, reject if `pal_id` is not in the set. Returns `ToolError::ExecutionFailed("pal not permitted")`.
4. Call `handler.execute(args)`. The handler is opaque — could be MCP or native.
5. Return the `ToolResult`.

The registry has no knowledge of MCP transports or native tool internals. It just dispatches to `Arc<dyn ToolHandler>`.

### `wisp-mcp` changes

`wisp-mcp` loses `ToolRegistry`, `ToolDefinition`, `ToolAnnotations` (moved to `wisp-tool-registry`). It keeps its protocol clients, managers, and transport types.

#### McpToolHandler

A new struct implementing `ToolHandler` that wraps MCP execution:

```rust
pub struct McpToolHandler {
    server_id: String,
    original_name: String,
    stdio_manager: Arc<McpStdioManager>,
    http_manager: Arc<McpHttpManager>,
    transport: TransportConfig,
}

#[async_trait]
impl ToolHandler for McpToolHandler {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        // dispatch to stdio or http manager based on transport
    }
}
```

#### Registration helper

`wisp-mcp` provides a convenience method that MCP server connection code calls after discovering tools:

```rust
pub fn register_mcp_tools(
    registry: &ToolRegistry,
    server_id: &str,
    tools: &[NormalizedTool],
    transport: &TransportConfig,
    stdio_manager: Arc<McpStdioManager>,
    http_manager: Arc<McpHttpManager>,
);
```

This iterates tools, builds `ToolDefinition` + `McpToolHandler` for each, and calls `registry.register()`. Replaces the current `ToolRegistry::register_server()`.

### Crate: `wisp-software-tools`

Provides the `NativeTool` trait, `SoftwareToolRegistry`, and built-in tool implementations.

#### NativeTool trait

Extends `ToolHandler` with metadata methods:

```rust
#[async_trait]
pub trait NativeTool: ToolHandler {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> Value;                          // schemars::schema_for!()
    fn requires_confirmation(&self) -> bool { false }   // override for destructive tools
    fn default_allowed_pals(&self) -> Vec<String> { vec![] }  // empty = unrestricted
}
```

Each tool is a unit struct with a dedicated args struct. The arg type is the single source of truth for the JSON schema:

```rust
#[derive(Deserialize, JsonSchema)]
pub struct GetSettingArgs { pub key: String }

pub struct GetSetting;

#[async_trait]
impl ToolHandler for GetSetting {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let args: GetSettingArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        // ... read setting ...
        Ok(ToolResult { content: vec![ToolContent::Text { text: value }], is_error: false })
    }
}

impl NativeTool for GetSetting {
    fn name(&self) -> &str { "wisp_setting_get" }
    fn description(&self) -> &str { "Read a setting by key" }
    fn schema(&self) -> Value { schemars::schema_for!(GetSettingArgs) }
}
```

#### SoftwareToolRegistry

Holds registered native tools and registers them into the shared `ToolRegistry`:

```rust
pub struct SoftwareToolRegistry {
    tools: HashMap<String, Arc<dyn NativeTool>>,
}

impl SoftwareToolRegistry {
    pub fn new() -> Self;
    pub fn register<T: NativeTool + 'static>(&mut self, tool: T) -> &mut Self;

    /// Register all native tools into the shared ToolRegistry.
    pub fn register_into(&self, registry: &ToolRegistry);
}
```

`register_into()` iterates tools, builds `ToolDefinition` (with `metadata.provider = "native"`), wraps each as `Arc<dyn NativeTool>` (which is also `Arc<dyn ToolHandler>`), and calls `registry.register()`.

#### NativeToolError

Internal to tool implementations. Converted to `ToolError` at the `ToolHandler` boundary:

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
```

#### Crate structure

```
crates/wisp-software-tools/
  Cargo.toml
  src/
    lib.rs
    trait.rs          # NativeTool trait
    registry.rs       # SoftwareToolRegistry
    error.rs          # NativeToolError
    tools/
      mod.rs
      settings/
        mod.rs
        get.rs
        set.rs
      js_exec.rs      # rquickjs-based execution (behind feature flag)
```

### Conversation engine integration

The conversation loop (`conversation_commands.rs`) calls `registry.execute()` as before, now with the added `pal_id` parameter sourced from the current chat context.

#### Confirmation gates

Confirmation is **not** checked in `execute()`. It's handled by the conversation engine before calling `execute()`:

1. Engine calls `registry.get_tool(name)` to inspect `requires_confirmation`.
2. If `true`, it emits a Tauri event `tool_needs_confirmation { stream_id, tool_name, arguments }` to the frontend.
3. The engine creates a `oneshot::Sender<ConfirmationDecision>` and stores it mapped by `stream_id` in `AppData`.
4. The frontend shows a dialog. The user clicks Approve/Reject.
5. Frontend calls a Tauri command `tool_confirm { stream_id, approved: bool }`.
6. The command resolves the oneshot channel.
7. The engine awaits the receiver. If approved, calls `execute()`. If rejected, inserts a tool result with `is_error: true` and text `"cancelled by user"`.

### AppData changes

`AppData` (`src-tauri/src/types.rs`) keeps `tool_registry: Arc<ToolRegistry>` but drops the direct `mcp_stdio_manager` / `mcp_http_manager` fields from the registry (they're now inside `McpToolHandler` instances). The managers remain in `AppData` for connection lifecycle management.

### Frontend changes

The frontend is Vue 3 + Naive UI + Pinia. Four areas need changes:

#### 1. Native tools in the tool list

Currently tools are grouped per MCP server in `McpServerDetails.vue` (filtered by `serverId`). Native tools have no server. We add a **virtual "Wisp" server entry** to `mcpStore.tools` — native tools get `serverId: "wisp"` and `metadata.provider: "native"`. This lets them flow through the existing tool list UI, display name enrichment, and enable/disable toggle with zero new components.

The `McpServerList.vue` shows a static "Wisp" entry — this is a UI-only concept, not a real `ServerConfig`. The store filters `tools` by `serverId === "wisp"` to populate the details view. No connect/disconnect button, no transport config shown for this entry. Clicking it opens `McpServerDetails.vue` showing native tools in the existing `n-data-table`.

#### 2. Per-tool enable/disable

Currently only whole-server toggle exists (Chat.vue popover `n-tag` click → `setServerEnabled`). We add **per-tool checkboxes** to the `McpServerDetails.vue` tool table — a new "Enabled" column with an `n-switch` per row. This calls a new store method `setToolEnabled(toolName, enabled)` which invokes a new Tauri command `registry_set_tool_enabled { name, enabled }`.

The Chat.vue popover remains server-level for quick toggling, but the details view gives granular control. Both MCP and native tools get per-tool toggles through the same UI.

#### 3. Confirmation dialog (human-in-the-loop)

**New event variant** in `ConversationEventPayload` (`src/libs/types.ts`):

```typescript
| { type: 'tool_needs_confirmation'; stream_id: string; tool_name: string; arguments: Record<string, unknown> }
```

**New component** `src/components/ToolConfirmDialog.vue`:
- Triggered when a `tool_needs_confirmation` event arrives.
- Shows: tool display name, description, formatted arguments (JSON), and a warning icon if the tool is destructive.
- Two buttons: "Approve" (primary) and "Reject" (default).
- Calls Tauri command `tool_confirm { stream_id, approved: bool }`.

**Store integration** (`src/stores/chat.ts`): each of the four `listenConversationEvents` call sites (send/regenerate/derive/edit-and-resend) gets a new branch for `tool_needs_confirmation` that opens the dialog and awaits the user's decision. The engine is blocked on the Rust side via the oneshot channel, so the frontend just needs to call `tool_confirm`.

**New Tauri command**: `tool_confirm(stream_id, approved)` — resolves the pending oneshot in `AppData`.

#### 4. Pal permission settings

In `McpServerDetails.vue`, the tool table gains a "Allowed Pals" column showing an `n-select` (multiple, tag mode) populated from the pals/characters store. Empty = unrestricted. Changing it calls a new store method `setToolAllowedPals(toolName, palIds)` which invokes a new Tauri command `registry_set_tool_pals { name, pal_ids }`.

This is only shown for native tools (`metadata.provider === "native"`) since MCP tools are unrestricted by default.

#### Summary of new Tauri commands

| Command | Purpose |
|---|---|
| `registry_set_tool_enabled { name, enabled }` | Per-tool enable/disable |
| `registry_set_tool_pals { name, pal_ids }` | Set allowed pals for a tool |
| `tool_confirm { stream_id, approved }` | Resolve confirmation gate |

#### Summary of new/changed frontend files

| File | Change |
|---|---|
| `src/libs/types.ts` | Add `tool_needs_confirmation` to `ConversationEventPayload`; add `requires_confirmation` to `RegisteredTool` |
| `src/libs/commands.ts` | Add `registrySetToolEnabled`, `registrySetToolPals`, `toolConfirm` |
| `src/stores/mcp.ts` | Add `setToolEnabled`, `setToolAllowedPals`; seed virtual "Wisp" server |
| `src/stores/chat.ts` | Handle `tool_needs_confirmation` event at all four listener sites |
| `src/components/ToolConfirmDialog.vue` | **New** — confirmation modal |
| `src/components/McpServerDetails.vue` | Add "Enabled" and "Allowed Pals" columns to tool table |
| `src/components/McpServerList.vue` | Add static "Wisp" entry |

### Built-in tools (initial set)

| Tool name | Description | Confirmation | Pal restriction |
|---|---|---|---|
| `wisp_setting_get` | Read a setting value | No | None |
| `wisp_setting_set` | Change a setting value | Yes | admin pal only |
| `js_exec` | Execute JavaScript (via rquickjs) | Yes | admin pal only |

### Cargo.toml dependencies

```toml
# workspace root
[workspace.dependencies]
wisp-tool-registry = { path = "crates/wisp-tool-registry" }
wisp-software-tools = { path = "crates/wisp-software-tools" }

# wisp-tool-registry
wisp-common.workspace = true
serde.workspace = true
serde_json.workspace = true
async-trait.workspace = true
thiserror.workspace = true
tokio = { workspace = true, features = ["sync"] }

# wisp-software-tools
wisp-common.workspace = true
wisp-tool-registry.workspace = true
serde.workspace = true
serde_json.workspace = true
schemars = "2"
async-trait.workspace = true
thiserror.workspace = true
rquickjs = { version = "0.9", optional = true }

# wisp-mcp (updated — gains wisp-tool-registry, loses tool_registry module)
wisp-tool-registry.workspace = true
```

## Migration path

1. Create `wisp-tool-registry` crate, move `ToolRegistry`, `ToolDefinition`, `ToolAnnotations` from `wisp-mcp`.
2. Move `ToolContent`, `ToolResult`, `ToolError` to `wisp-common`.
3. Add `ToolHandler` trait to `wisp-tool-registry`. Implement `McpToolHandler` in `wisp-mcp`.
4. Update `ToolRegistry::execute()` to use `Arc<dyn ToolHandler>` + pal-aware signature.
5. Create `wisp-software-tools` crate with `NativeTool` trait + `SoftwareToolRegistry`.
6. Wire up in `src-tauri` (AppData, setup, commands).
7. Frontend changes.

## Non-goals

- Dynamic plugin loading (tools are compiled in)
- Language runtimes beyond JS (rquickjs is behind a feature flag)

## Future considerations

- The `NativeTool` trait is the extension point for any new tool. Adding a Python execution tool later means implementing `NativeTool` for a struct that spawns a subprocess — no registry changes.
- The `ToolHandler` trait makes it trivial to add new tool providers beyond MCP and native (e.g., a remote RPC provider).

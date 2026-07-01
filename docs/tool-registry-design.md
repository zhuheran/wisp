# Tool Registry 设计

## 动机

当前 tool call 的架构中，MCP server 的连接、工具发现、命名、执行全都耦合在一起。
上层（Conversation Engine）需要知道 `serverId:toolName` 这类 qualified name，
前端也需要区分 stdio/http  transport 来调用不同的 Tauri command。

目标是把**注册**和**执行**抽象成一层，上层只和 `ToolRegistry` 交互：

```
AI / Conversation Engine
        │
        ▼
   ToolRegistry         ← 唯一的入口
   ├── execute(name, args) → Future<Result>
   ├── list_tools() → ToolDefinition[]
   └── get_tool(name) → ToolDefinition
        │
        ├── MCP Executor (stdio/http, 对上层透明)
        ├── 未来: Local Executor
        └── 未来: Remote API Executor
```

## 核心类型

```rust
// ===== tool_registry/types.rs =====

/// 注册后的工具定义——上层看到的完整形态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// 注册名, e.g. "mcp_tavily_search", "mcp_filesystem_read"
    /// `mcp_` 前缀由 registry 层添加，标识来自 MCP 提供
    /// 这是 AI 在 <|tool_calls|> 中使用的名字
    pub name: String,
    pub description: Option<String>,
    /// JSON Schema, 类型为 { type: "object", properties: {...} }
    pub input_schema: serde_json::Value,
    pub annotations: Option<ToolAnnotations>,
    /// 额外元数据, 执行器可自定义
    pub metadata: HashMap<String, serde_json::Value>,
    /// 是否启用
    pub enabled: bool,
}

/// 执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: Vec<ToolContent>,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolContent {
    Text { text: String },
    Image { data: String, mime_type: String },
    Resource { uri: String, mime_type: Option<String>, text: Option<String>, blob: Option<String> },
}

/// 注册名生成规则: "mcp_" + serverId + "_" + originalName
/// 只保留字母数字和下划线，`mcp_` 前缀标识 MCP 提供者
/// e.g. (tavily, search) → "mcp_tavily_search"
pub fn registered_name(server_id: &str, tool_name: &str) -> String {
    let clean = |s: &str| -> String {
        s.chars()
            .map(|ch| if ch.is_ascii_alphanumeric() || ch == '_' { ch } else { '_' })
            .collect::<String>()
            .trim_matches('_')
            .to_string()
    };
    format!("mcp_{}_{}", clean(server_id), clean(tool_name))
        .to_lowercase()
}
```

## ToolRegistry

```rust
// ===== tool_registry/registry.rs =====

pub struct ToolRegistry {
    /// 注册名 → 工具条目
    entries: HashMap<String, ToolEntry>,
    /// 已启用的注册名集合
    enabled: HashSet<String>,
    // MCP 执行依赖 (对上层透明)
    stdio_manager: Arc<McpStdioManager>,
    http_manager: Arc<McpHttpManager>,
}

struct ToolEntry {
    definition: ToolDefinition,
    // 以下字段对上层透明，仅执行器使用
    server_id: String,
    original_name: String,
    transport: TransportConfig,
}
```

### 公开 API

```rust
impl ToolRegistry {
    // ==== 注册/注销 ====

    /// MCP server 连接后，注册其所有工具
    pub fn register_server(&mut self, server_id: &str, tools: &[RawMcpTool], transport: &TransportConfig)

    /// MCP server 断开后，注销其所有工具
    pub fn unregister_server(&mut self, server_id: &str)

    // ==== 查询 ====

    /// 列出所有已注册的工具定义（用于前端展示 / AI prompt）
    pub fn list_tools(&self) -> Vec<&ToolDefinition>

    /// 列出已启用的工具（用于构建 OpenAI native tools / prompt）
    pub fn list_enabled_tools(&self) -> Vec<&ToolDefinition>

    /// 按注册名查找
    pub fn get_tool(&self, name: &str) -> Option<&ToolDefinition>

    /// 获取已启用的注册名集合
    pub fn enabled_set(&self) -> &HashSet<String>

    /// 设置启用的工具
    pub fn set_enabled(&mut self, names: HashSet<String>)

    // ==== 执行 ====

    /// 按注册名执行工具, 返回 Future
    /// 上层不需要知道是 MCP stdio 还是 http
    pub async fn execute(&self, name: &str, args: serde_json::Value) -> Result<ToolResult, ToolError>

    // ==== Prompt 生成 ====

    /// 生成 AI prompt 中的 tools 章节
    pub fn build_tools_prompt(&self) -> String

    /// 生成 OpenAI native `ChatCompletionTool` 列表
    pub fn build_provider_tools(&self) -> Vec<ChatCompletionTool>
}
```

### 执行流程

```
execute("mcp_tavily_search", {"query": "weather"})
  → lookup entries["mcp_tavily_search"]
  → get server_id="tavily", original_name="search", transport=Stdio{...}
  → match transport:
      Stdio => stdio_manager.call_tool("tavily", "search", args)
      Sse | Http => http_manager.call_tool("tavily", "search", args)
  → normalize result → ToolResult
```

### native OpenAI tool calling 的 name 映射

当使用 OpenAI 原生 tool calling 时，注册名直接作为 FunctionObject.name：

```rust
ChatCompletionTool {
    r#type: ChatCompletionToolType::Function,
    function: FunctionObject {
        name: "mcp_tavily_search",  // 直接使用注册名
        description: ...,
        parameters: ...,
    },
}
```

模型返回的 `tool_calls[i].function.name` 就是 `"mcp_tavily_search"`。
`execute("mcp_tavily_search", ...)` 直接按这个名字查找。

### 启用/禁用的数据结构变化

当前: `enabled_tools: HashSet<String>` 存的是 `qualified_name` (`"tavily:search"`)
新:   `enabled_tools: HashSet<String>` 存的是 `registered_name` (`"mcp_tavily_search"`)

这样前端只需要传注册名，不需要知道 `qualified_name`。

## Tauri Command 接口

替换掉现有的 MCP 工具相关的 command，新增 Registry 级别的 command：

### 新增

```rust
// 刷新所有已连接 server 的工具注册
#[tauri::command]
async fn registry_refresh(app: AppHandle) -> Result<(), String>

// 列出所有已注册工具（前端展示用）
#[tauri::command]
async fn registry_list_tools(app: AppHandle) -> Result<Vec<ToolDefinition>, String>

// 执行工具（前端 tool call 回环用）
#[tauri::command]
async fn registry_execute(app: AppHandle, name: String, arguments: Option<Value>) -> Result<ToolResult, String>

// 设置启用状态
#[tauri::command]
async fn registry_set_enabled(app: AppHandle, names: Vec<String>) -> Result<(), String>
```

### 移除（不再需要前端直接调用）

- `mcp_list_global_tools` → 替代为 `registry_list_tools`
- `mcp_set_global_enabled_tools` → 替代为 `registry_set_enabled`
- `mcp_refresh_global_tool_state` → 替代为 `registry_refresh`
- `mcp_set_server_enabled` → 合并到 `registry_set_enabled`
- `mcp_stdio_call_tool` / `mcp_http_call_tool` → 替代为 `registry_execute`

## 文件结构

```
src-tauri/src/
├── tool_registry/
│   ├── mod.rs              # 公开 API 重导出
│   ├── types.rs            # ToolDefinition, ToolResult, ToolContent, ToolError
│   └── registry.rs         # ToolRegistry 实现
├── types.rs                # AppData: GlobalMcpToolState → ToolRegistry
├── lib.rs                  # 初始化, 注册新 command
├── conversation/
│   └── commands.rs         # 用 registry.execute 替换 qualified_name 解析
└── mcp/
    └── commands.rs         # register_server/unregister_server 在连接/断开时调用
```

## 迁移步骤

1. 创建 `tool_registry/types.rs` — 类型定义
2. 创建 `tool_registry/registry.rs` — ToolRegistry 实现
3. 创建 `tool_registry/mod.rs` — 重导出
4. 修改 `types.rs` — 替换 `GlobalMcpToolState` 为 `ToolRegistry`
5. 修改 `lib.rs` — 初始化 ToolRegistry, 注册新 command
6. 修改 `mcp/commands.rs` — 连接时 register, 断开时 unregister
7. 修改 `conversation/commands.rs` — 用 registry 替代 direct MCP calls
8. 修改前端 `stores/mcp.ts` — 用 registry commands 替代 MCP-specific commands

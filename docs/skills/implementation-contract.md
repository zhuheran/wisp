# Agent Skills Support — Implementation Contract

目标：在 wisp-pro 中支持 [Agent Skills 开放标准](https://agentskills.io/specification)（SKILL.md + frontmatter + 渐进式披露）。

## 1. 新 crate: `wisp-skills`（Rust，TDD 开发）

位于 `crates/wisp-skills/`，加入 workspace（`Cargo.toml` members + workspace.dependencies 加 `wisp-skills = { path = "crates/wisp-skills" }`）。

依赖：`wisp-common`（`ToolContent, ToolError, ToolResult`）、`wisp-software-tools`（`NativeTool` trait）、`serde`、`serde_json`（workspace 依赖）。**不引入 serde_yaml**——frontmatter 手写极简解析。

### 1.1 数据模型

```rust
pub struct Skill {
    pub name: String,          // frontmatter name，必须与目录名一致
    pub description: String,   // frontmatter description
    pub license: Option<String>,
    pub compatibility: Option<String>,
    pub allowed_tools: Vec<String>,  // frontmatter allowed-tools（空格分隔），V1 仅解析不执行
    pub path: PathBuf,         // skill 目录绝对路径
    pub body: String,          // SKILL.md 正文（frontmatter 之后的内容）
}
```

### 1.2 错误类型

```rust
#[derive(Debug, Error)]
pub enum SkillError {
    Io(#[from] io::Error),
    MissingFrontmatter,                    // 没有 --- 包裹的 frontmatter
    InvalidName(String),                   // name 不合规
    NameMismatch { dir: String, name: String },  // name != 目录名
    MissingDescription,
    DescriptionTooLong(usize),             // > 1024
    CompatibilityTooLong(usize),           // > 500
    NoSkillMd(PathBuf),                    // 目录里没有 SKILL.md
}
```

### 1.3 API

```rust
// 加载单个 skill 目录（校验：name 与目录名一致、description 非空等）
pub fn load_skill(dir: &Path) -> Result<Skill, SkillError>;

// 扫描目录下的所有 skill 子目录；宽容模式——单目录失败不影响其他
pub fn load_skills(dir: &Path) -> (Vec<Skill>, Vec<(String, SkillError)>);

// L1 元数据段落（注入 system prompt）：name + description 列表
pub fn assemble_skills_prompt(skills: &[Skill]) -> String;
// 空列表返回空字符串（与 build_tools_prompt 行为一致，调用方 `if !is_empty` 判断）
```

### 1.4 frontmatter 解析规则（`---` 包裹，YAML 标量子集）

```
---
name: xxx
description: xxx
license: xxx        # 可选
compatibility: xxx  # 可选，≤500
allowed-tools: a b  # 可选，空格分隔
metadata:           # 可选，V1 不解析（跳过该键及其缩进子行）
  author: xx
---
```

- 解析器只处理 `key: value` 标量行；`metadata:` 后的缩进行跳过；未知键忽略
- 值两侧空白 trim；值中冒号后多余内容保留原样（如 URL）
- 无 frontmatter 或前后 `---` 缺失 → `MissingFrontmatter`
- name 校验：1-64 字符，仅 `[a-z0-9-]`，不以 `-` 开头/结尾，无连续 `--`

### 1.5 `LoadSkillTool`（NativeTool 实现，同 crate，**单工具设计**）

一个 `load_skill` 工具承载所有已启用 skill，通过 `skill_name` 参数选择（避免每个 skill 一个工具导致工具列表膨胀、逼近 API 工具数量上限；skill 是"加载内容"而非"执行动作"）。触发流程（渐进式披露）：system prompt 里的 L1 清单（name + description）让模型决定加载哪个 skill → 调用 `load_skill(skill_name)` → 正文作为工具结果进入上下文。

```rust
pub struct LoadSkillTool { skills: Vec<Skill> }

impl LoadSkillTool {
    pub const TOOL_NAME: &'static str = "load_skill";  // 符合 ^[a-zA-Z0-9_-]+$
    pub fn new(skills: Vec<Skill>) -> Self;
}
```

- `name()` 返回常量 `"load_skill"`（不随 skill 集合变化）
- `description()`：说明从 system prompt 的 `Available skills` 清单取名字
- `schema()`：`{ skill_name: { type: string, enum: [已启用 skill 名...] }, required: [skill_name] }`——enum 随启用状态动态生成
- `run()`：按 `skill_name` 返回对应正文；未知名 → `ToolError::NotFound`；缺参 → `ToolError::ExecutionFailed`
- `format_to_text`/`format_to_markdown`：按参数中的 `skill_name` 返回正文
- 无状态（幂等/去重由上层决定）

### 1.6 测试要求（TDD，red-green-refactor）

必须覆盖：
- 解析：完整 frontmatter、缺失 description、超长 description/compatibility、无 frontmatter、metadata 跳过、未知键忽略、值含冒号（URL）、allowed-tools 解析
- 校验：name 非法字符/大小写/首尾连字符/连续连字符、name 与目录名不一致、SKILL.md 缺失、目录不存在
- `load_skills`：混合目录（好+坏）宽容返回
- `assemble_skills_prompt`：空列表返回空、多个 skill 格式、含 name+description
- `SkillLoadTool`：name/description/schema/run 返回正文

测试用临时目录（`std::env::temp_dir()` + uuid，参照 `src-tauri` 测试模式；或 `tempfile` 若已是依赖）。

## 2. Tauri 命令（integration 部分，主 agent 实现）

```rust
#[derive(Serialize)] pub struct SkillInfo { name, description, enabled, path }  // snake_case 序列化
skills_list(app_handle) -> Vec<SkillInfo>
skills_refresh(app_handle) -> Vec<SkillInfo>   // 重新扫描目录 + 更新注册
skills_toggle(app_handle, name: String) -> Vec<SkillInfo>
skills_open_folder(app_handle) -> Result<(), String>  // 打开系统文件管理器
```

- skills 扫描目录（按优先级）：`app_data_dir()/skills`（应用自有，可写，打开文件夹命令指向这里）+ `~/.agents/skills`（全局 Agent Skills 目录，只读扫描，与 Claude Code/Zed 共享）；同名 skill 应用目录优先
- 注册进 `ToolRegistry`：**单个** `load_skill` 工具（`LoadSkillTool::TOOL_NAME`），enum 只含启用的 skill；per-skill 启用状态存 `AppData.enabled_skills`，refresh 保留启用状态、新 skill 默认启用
- `skills_list` 返回按 name 排序

## 3. 前端（子 agent 实现，不需要测试）

### 3.1 TS 接口（`src/libs/types.ts` 追加）

```ts
export interface SkillInfo {
  name: string
  description: string
  enabled: boolean
  path: string
}
```

### 3.2 命令封装（`src/libs/commands.ts` 追加）

```ts
skillsList(): Promise<SkillInfo[]>        // invoke('skills_list')
skillsRefresh(): Promise<SkillInfo[]>     // invoke('skills_refresh')
skillsToggle(name: string): Promise<SkillInfo[]>  // invoke('skills_toggle', { name })
skillsOpenFolder(): Promise<void>         // invoke('skills_open_folder')
```

### 3.3 页面与 store

- `src/stores/skills.ts`：pinia store（state: `skills: SkillInfo[]`；actions: `init/list/refresh/toggle`；参照 `src/stores/mcp.ts` 风格）
- `src/views/SkillsView.vue`：列表（name + description + path + 启用开关 `n-switch`）、「打开 skills 文件夹」按钮、「重新扫描」按钮、空态提示（"将 SKILL.md 目录放入 skills 文件夹后点击重新扫描"）
- `src/router/index.ts` 加 `/skills` 路由；`src/App.vue` 侧边栏加入口（图标参照 McpView 入口写法）+ provide SkillsStore
- 样式参照 `src/views/McpView.vue`（卡片/列表 + n-button 风格），不引入新依赖

## 4. Integration（主 agent 实现）

- `src-tauri/src/lib.rs`：AppData 无新字段；启动时扫描 skills 目录 → 注册 SkillLoadTool；命令注册
- `src-tauri/src/conversation_commands.rs`：`system_prompt_sections` 在 Pal prompt 之后、工具说明之前插入 `assemble_skills_prompt` 段（仅启用中的 skill）
- `src-tauri/src/orchestrator.rs`：`build_context_for_pal` 同样注入（保持 Pal 与主路径一致）

## 5. 验证命令

- Rust: `cargo test -p wisp-skills`（TDD 循环内）、完成后 `cargo test --workspace`
- 前端: `npx vue-tsc --noEmit`、`npx vitest run`（不新增前端测试，仅确认不破坏现有）

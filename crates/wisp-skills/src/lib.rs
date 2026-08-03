//! # wisp-skills
//!
//! [Agent Skills 开放标准](https://agentskills.io/specification) 支持：
//! 读取 `SKILL.md`（frontmatter + 正文）、校验、L1 元数据段落拼装，
//! 以及暴露正文给模型的 `SkillLoadTool`（`NativeTool` 实现）。
//!
//! 契约：`docs/skills/implementation-contract.md` 第 1 节。frontmatter 手写
//! 极简解析（YAML 标量子集），不引入 serde_yaml。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;
use wisp_common::{ToolContent, ToolError, ToolResult};
use wisp_software_tools::NativeTool;

/// 单个 skill：frontmatter 字段 + SKILL.md 正文。
#[derive(Debug, Clone)]
pub struct Skill {
    /// frontmatter `name`，必须与目录名一致。
    pub name: String,
    /// frontmatter `description`。
    pub description: String,
    /// frontmatter `license`（可选）。
    pub license: Option<String>,
    /// frontmatter `compatibility`（可选，≤500 字符）。
    pub compatibility: Option<String>,
    /// frontmatter `allowed-tools`（空格分隔）；V1 仅解析不执行。
    pub allowed_tools: Vec<String>,
    /// skill 目录绝对路径。
    pub path: PathBuf,
    /// SKILL.md 正文（frontmatter 之后的内容）。
    pub body: String,
    /// skill 目录内资源文件的相对路径（递归扫描，排除 SKILL.md，排序）。
    pub resources: Vec<String>,
}

/// skill 加载/校验错误。
#[derive(Debug, Error)]
pub enum SkillError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    /// 没有 `---` 包裹的 frontmatter。
    #[error("missing frontmatter: SKILL.md must start and end with a --- block")]
    MissingFrontmatter,
    /// name 不合规（1-64 字符、仅 `[a-z0-9-]`、不以 `-` 开头/结尾、无连续 `--`）。
    #[error("invalid skill name {0:?}")]
    InvalidName(String),
    /// name 与目录名不一致。
    #[error("skill name {name:?} does not match directory name {dir:?}")]
    NameMismatch { dir: String, name: String },
    #[error("skill description is missing")]
    MissingDescription,
    #[error("skill description is too long: {0} chars (max 1024)")]
    DescriptionTooLong(usize),
    #[error("compatibility is too long: {0} chars (max 500)")]
    CompatibilityTooLong(usize),
    /// 目录里没有 SKILL.md。
    #[error("no SKILL.md found in {0}")]
    NoSkillMd(PathBuf),
}

/// 解析出的 frontmatter 字段（校验前的原始形态）。
struct ParsedFrontmatter {
    name: Option<String>,
    description: Option<String>,
    license: Option<String>,
    compatibility: Option<String>,
    allowed_tools: Vec<String>,
}

impl Default for ParsedFrontmatter {
    fn default() -> Self {
        Self {
            name: None,
            description: None,
            license: None,
            compatibility: None,
            allowed_tools: Vec::new(),
        }
    }
}

/// 解析 SKILL.md 内容：`---` 包裹的 frontmatter + 正文。
///
/// 规则（契约 1.4）：只处理 `key: value` 标量行；`metadata:` 后的缩进行跳过；
/// 未知键忽略；值两侧 trim；值中冒号保留原样（如 URL）。
fn parse_frontmatter(content: &str) -> Result<(ParsedFrontmatter, String), SkillError> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.first().map(|l| l.trim()) != Some("---") {
        return Err(SkillError::MissingFrontmatter);
    }
    let closing_idx = lines[1..]
        .iter()
        .position(|l| l.trim() == "---")
        .map(|i| i + 1)
        .ok_or(SkillError::MissingFrontmatter)?;

    let mut fm = ParsedFrontmatter::default();
    for line in &lines[1..closing_idx] {
        // 空行、注释行、缩进行（metadata 子行）跳过。
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue; // 非 `key: value` 行，忽略
        };
        let key = key.trim();
        let value = value.trim().to_string();
        match key {
            "name" => fm.name = Some(value),
            "description" => fm.description = Some(value),
            "license" => fm.license = Some(value),
            "compatibility" => fm.compatibility = Some(value),
            "allowed-tools" => {
                fm.allowed_tools = value.split_whitespace().map(String::from).collect();
            },
            // `metadata:` 的键值被忽略；其缩进子行由上面的缩进规则跳过。
            "metadata" => {},
            // 未知键忽略。
            _ => {},
        }
    }

    let body = lines[closing_idx + 1..].join("\n");
    Ok((fm, body))
}

/// 校验 name：1-64 字符，仅 `[a-z0-9-]`，不以 `-` 开头/结尾，无连续 `--`。
fn validate_name(name: &str) -> Result<(), SkillError> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--");
    if ok {
        Ok(())
    } else {
        Err(SkillError::InvalidName(name.to_string()))
    }
}

/// 加载单个 skill 目录（校验：name 与目录名一致、description 非空等）。
pub fn load_skill(dir: &Path) -> Result<Skill, SkillError> {
    // 目录不存在/不可读 → 真实 IO 错误（区别于 NoSkillMd：目录存在但无 SKILL.md）。
    fs::read_dir(dir)?;
    let skill_md_path = dir.join("SKILL.md");
    if !skill_md_path.is_file() {
        return Err(SkillError::NoSkillMd(dir.to_path_buf()));
    }
    let content = fs::read_to_string(&skill_md_path)?;
    let (fm, body) = parse_frontmatter(&content)?;

    let name = fm.name.unwrap_or_default();
    validate_name(&name)?;

    let dir_name = dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();
    if dir_name != name {
        return Err(SkillError::NameMismatch { dir: dir_name, name });
    }

    let description = fm.description.unwrap_or_default();
    if description.is_empty() {
        return Err(SkillError::MissingDescription);
    }
    let desc_len = description.chars().count();
    if desc_len > 1024 {
        return Err(SkillError::DescriptionTooLong(desc_len));
    }

    if let Some(compat) = &fm.compatibility {
        let compat_len = compat.chars().count();
        if compat_len > 500 {
            return Err(SkillError::CompatibilityTooLong(compat_len));
        }
    }

    Ok(Skill {
        name,
        description,
        license: fm.license.filter(|s| !s.is_empty()),
        compatibility: fm.compatibility.filter(|s| !s.is_empty()),
        allowed_tools: fm.allowed_tools,
        path: dir.to_path_buf(),
        body,
        resources: scan_resources(dir),
    })
}

/// 递归收集 skill 目录内的资源文件（相对路径，`/` 分隔，排序），
/// 排除 `SKILL.md`、隐藏文件（`.` 开头，如 `.DS_Store`）和缓存目录
/// （`__pycache__`）。
fn scan_resources(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, current: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(current) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            if name.starts_with('.') || name == "__pycache__" {
                continue; // 隐藏文件 / 缓存目录
            }
            if path.is_dir() {
                walk(base, &path, out);
            } else if name != "SKILL.md" {
                if let Ok(rel) = path.strip_prefix(base) {
                    out.push(rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"));
                }
            }
        }
    }

    let mut resources = Vec::new();
    walk(dir, dir, &mut resources);
    resources.sort();
    resources
}

/// 扫描 `dir` 下的所有 skill 子目录；宽容模式——单目录失败不影响其他。
///
/// 返回 (加载成功的 skills, 失败项 (目录名, 错误))。根目录读取失败时返回空结果。
pub fn load_skills(dir: &Path) -> (Vec<Skill>, Vec<(String, SkillError)>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return (Vec::new(), Vec::new()),
    };

    let mut skills = Vec::new();
    let mut errors = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue; // 根目录下的普通文件不是 skill
        }
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        match load_skill(&path) {
            Ok(skill) => skills.push(skill),
            Err(e) => errors.push((dir_name, e)),
        }
    }
    (skills, errors)
}

/// L1 元数据段落（注入 system prompt）：name + description 列表。
/// 空列表返回空字符串（调用方以 `!is_empty` 判断是否注入）。
pub fn assemble_skills_prompt(skills: &[Skill]) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let mut lines = vec!["Available skills:".to_string()];
    for skill in skills {
        lines.push(format!("- {}: {}", skill.name, skill.description));
    }
    lines.join("\n")
}

/// 把 skill 正文暴露为可调用工具的 `NativeTool` 实现（单例）。
///
/// 设计：**一个** `load_skill` 工具承载所有已启用 skill，通过 `skill_name`
/// 参数选择——每个 skill 一个工具的方案会让工具列表随 skill 数量膨胀
/// （逼近 API 工具上限），且 skill 是"加载内容"而非"执行动作"，与工具语义
/// 不符。触发流程（渐进式披露）：system prompt 里的 L1 清单
/// （name + description）让模型决定加载哪个 skill → 调用
/// `load_skill(skill_name)` → 正文作为工具结果进入上下文。无状态，
/// 幂等/去重由上层决定。
pub struct LoadSkillTool {
    skills: Vec<Skill>,
}

impl LoadSkillTool {
    /// 工具名（常量，不随 skill 集合变化；符合 `^[a-zA-Z0-9_-]+$`）。
    pub const TOOL_NAME: &'static str = "load_skill";

    pub fn new(skills: Vec<Skill>) -> Self {
        Self { skills }
    }

    fn find_body(&self, name: &str) -> Option<&str> {
        self.skills.iter().find(|s| s.name == name).map(|s| s.body.as_str())
    }
}

#[async_trait]
impl NativeTool for LoadSkillTool {
    fn name(&self) -> &str {
        Self::TOOL_NAME
    }

    fn description(&self) -> &str {
        "Load a skill's instructions by name. Available skills (name and when \
to use them) are listed in the system prompt under \"Available skills\". \
Pass the exact name as the skill_name argument."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill_name": {
                    "type": "string",
                    "enum": self.skills.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
                    "description": "The name of the skill to load",
                }
            },
            "required": ["skill_name"],
        })
    }

    async fn run(&self, args: Value) -> Result<ToolResult, ToolError> {
        let name = args
            .get("skill_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::ExecutionFailed("missing 'skill_name' argument".to_string())
            })?;
        let skill = self
            .skills
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| ToolError::NotFound(name.to_string()))?;
        Ok(ToolResult {
            content: vec![ToolContent::Text { text: load_result_text(skill) }],
            is_error: false,
        })
    }

    fn format_to_text(
        &self,
        _name: &str,
        arguments: &Value,
        _result: Option<&ToolResult>,
    ) -> String {
        arguments
            .get("skill_name")
            .and_then(|v| v.as_str())
            .and_then(|n| self.skills.iter().find(|s| s.name == n))
            .map(load_result_text)
            .unwrap_or_default()
    }

    fn format_to_markdown(
        &self,
        _name: &str,
        arguments: &Value,
        _result: Option<&ToolResult>,
    ) -> String {
        arguments
            .get("skill_name")
            .and_then(|v| v.as_str())
            .and_then(|n| self.skills.iter().find(|s| s.name == n))
            .map(load_result_text)
            .unwrap_or_default()
    }
}

/// 正文 + 资源清单（渐进式披露 L3：模型从清单里决定按需读取哪个文件）。
fn load_result_text(skill: &Skill) -> String {
    let mut out = skill.body.clone();
    if !skill.resources.is_empty() {
        out.push_str("\n\n## Skill resources\n");
        for resource in &skill.resources {
            out.push_str(&format!("- {resource}\n"));
        }
    }
    out
}

/// 读取 skill 目录内资源文件的 `NativeTool` 实现（单例）。
///
/// 渐进式披露 L3：`load_skill` 返回正文 + 资源清单后，模型用
/// `read_skill_resources(skill_name, path)` 按需读取具体文件（
/// references/scripts/assets 等）。安全约束：路径必须解析在 skill
/// 目录内（防目录穿越），只读 UTF-8 文本（脚本按文本阅读、不执行），
/// 单文件 ≤ [`MAX_RESOURCE_SIZE`] 字节。
pub struct ReadSkillResourcesTool {
    skills: Vec<Skill>,
}

/// 单文件大小上限（512 KiB）——防止超大文件撑爆工具结果/上下文。
pub const MAX_RESOURCE_SIZE: u64 = 512 * 1024;

impl ReadSkillResourcesTool {
    /// 工具名（常量，符合 `^[a-zA-Z0-9_-]+$`）。
    pub const TOOL_NAME: &'static str = "read_skill_resources";

    pub fn new(skills: Vec<Skill>) -> Self {
        Self { skills }
    }
}

#[async_trait]
impl NativeTool for ReadSkillResourcesTool {
    fn name(&self) -> &str {
        Self::TOOL_NAME
    }

    fn description(&self) -> &str {
        "Read a resource file inside a skill's directory (references/, scripts/, \
assets/). The file list is returned by load_skill. Pass the skill_name and \
the resource path exactly as listed."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill_name": {
                    "type": "string",
                    "enum": self.skills.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
                    "description": "The name of the skill the resource belongs to",
                },
                "path": {
                    "type": "string",
                    "description": "Relative path of the resource file inside the skill directory",
                },
            },
            "required": ["skill_name", "path"],
        })
    }

    async fn run(&self, args: Value) -> Result<ToolResult, ToolError> {
        let name = args
            .get("skill_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::ExecutionFailed("missing 'skill_name' argument".to_string())
            })?;
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::ExecutionFailed("missing 'path' argument".to_string()))?;
        let skill = self
            .skills
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| ToolError::NotFound(name.to_string()))?;

        let text = read_resource(&skill.path, path)?;
        Ok(ToolResult {
            content: vec![ToolContent::Text { text }],
            is_error: false,
        })
    }
}

/// 读取 skill 目录内相对路径 `rel` 的文本文件。
///
/// 校验：相对路径 + canonicalize 后必须仍在 skill 目录内（防 `..` 穿越）；
/// 必须是 UTF-8 文本；大小 ≤ [`MAX_RESOURCE_SIZE`]。
fn read_resource(base: &Path, rel: &str) -> Result<String, ToolError> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err(ToolError::ExecutionFailed(format!(
            "resource path must be relative: {rel}"
        )));
    }
    let base_canon = base.canonicalize().map_err(|e| {
        ToolError::ExecutionFailed(format!("skill directory unavailable: {e}"))
    })?;
    let candidate = base_canon.join(rel_path);
    let candidate_canon = candidate.canonicalize().map_err(|_| {
        ToolError::ExecutionFailed(format!("resource not found: {rel}"))
    })?;
    if !candidate_canon.starts_with(&base_canon) {
        return Err(ToolError::ExecutionFailed(format!(
            "resource path escapes the skill directory: {rel}"
        )));
    }
    let len = std::fs::metadata(&candidate_canon).map_err(|e| {
        ToolError::ExecutionFailed(format!("resource unreadable: {e}"))
    })?.len();
    if len > MAX_RESOURCE_SIZE {
        return Err(ToolError::ExecutionFailed(format!(
            "resource exceeds {MAX_RESOURCE_SIZE} bytes: {rel}"
        )));
    }
    std::fs::read_to_string(&candidate_canon).map_err(|e| {
        ToolError::ExecutionFailed(format!("resource is not readable text: {e}"))
    })
}

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
    })
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

/// 把 skill 正文暴露为可调用工具的 `NativeTool` 实现。
///
/// 工具名 `skill:{name}`；调用返回 SKILL.md 正文（渐进式披露：模型先见
/// description，调用后才读到正文）。无状态，幂等/去重由上层决定。
pub struct SkillLoadTool {
    name: String,
    description: String,
    body: String,
}

impl SkillLoadTool {
    pub fn new(skill: &Skill) -> Self {
        Self {
            name: format!("skill:{}", skill.name),
            description: skill.description.clone(),
            body: skill.body.clone(),
        }
    }
}

#[async_trait]
impl NativeTool for SkillLoadTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        // description 原样（规范要求 description 已含"做什么+何时用"）。
        &self.description
    }

    fn schema(&self) -> Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    async fn run(&self, _args: Value) -> Result<ToolResult, ToolError> {
        Ok(ToolResult {
            content: vec![ToolContent::Text { text: self.body.clone() }],
            is_error: false,
        })
    }

    fn format_to_text(
        &self,
        _name: &str,
        _arguments: &Value,
        _result: Option<&ToolResult>,
    ) -> String {
        // 返回正文原文（默认 formatter 会加 `[name] success` 头，不符合契约）。
        self.body.clone()
    }

    fn format_to_markdown(
        &self,
        _name: &str,
        _arguments: &Value,
        _result: Option<&ToolResult>,
    ) -> String {
        self.body.clone()
    }
}

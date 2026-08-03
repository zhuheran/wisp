//! ReadSkillResourcesTool（NativeTool 实现）测试：读取 skill 目录内的
//! 资源文件（references/scripts/assets，渐进式披露 L3）。

use std::path::PathBuf;

use wisp_common::{ToolContent, ToolError};
use wisp_skills::{ReadSkillResourcesTool, Skill};
use wisp_software_tools::NativeTool;

fn skill_with_resources() -> Skill {
    Skill {
        name: "pdf-processing".to_string(),
        description: "Extract PDF text. Use when handling PDFs.".to_string(),
        license: None,
        compatibility: None,
        allowed_tools: Vec::new(),
        path: PathBuf::from("/tmp/skills/pdf-processing"),
        body: "# PDF Processing\n\nSee references/REFERENCE.md.".to_string(),
        resources: vec![
            "references/REFERENCE.md".to_string(),
            "scripts/extract.py".to_string(),
        ],
    }
}

fn tool() -> ReadSkillResourcesTool {
    ReadSkillResourcesTool::new(vec![skill_with_resources()])
}

#[test]
fn tool_name_is_read_skill_resources() {
    let tool = tool();
    assert_eq!(tool.name(), "read_skill_resources");
}

#[test]
fn tool_name_matches_openai_pattern() {
    let tool = tool();
    let name = tool.name();
    assert!(
        !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "tool name {name:?} must match ^[a-zA-Z0-9_-]+$"
    );
}

#[test]
fn tool_schema_enumerates_skills_and_requires_path() {
    let tool = tool();
    let schema = tool.schema();
    assert_eq!(schema["properties"]["skill_name"]["enum"], serde_json::json!(["pdf-processing"]));
    assert_eq!(schema["properties"]["path"]["type"], "string");
    assert_eq!(schema["required"], serde_json::json!(["skill_name", "path"]));
}

#[tokio::test]
async fn run_reads_text_file_inside_skill_dir() {
    let dir = std::env::temp_dir().join(format!("wisp-res-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(dir.join("references")).unwrap();
    std::fs::write(dir.join("references/REFERENCE.md"), "reference content").unwrap();
    let mut skill = skill_with_resources();
    skill.path = dir.clone();
    let tool = ReadSkillResourcesTool::new(vec![skill]);

    let result = tool
        .run(serde_json::json!({
            "skill_name": "pdf-processing",
            "path": "references/REFERENCE.md",
        }))
        .await
        .expect("run succeeds");

    let _ = std::fs::remove_dir_all(&dir);
    assert!(!result.is_error);
    assert_eq!(result.content[0], ToolContent::Text { text: "reference content".to_string() });
}

#[tokio::test]
async fn run_rejects_path_traversal() {
    let dir = std::env::temp_dir().join(format!("wisp-res-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("secret.txt"), "secret").unwrap();
    let mut skill = skill_with_resources();
    skill.path = dir.clone();
    let tool = ReadSkillResourcesTool::new(vec![skill]);

    let err = tool
        .run(serde_json::json!({
            "skill_name": "pdf-processing",
            "path": "../secret.txt",
        }))
        .await
        .expect_err("path traversal must error");

    let _ = std::fs::remove_dir_all(&dir);
    assert!(matches!(err, ToolError::ExecutionFailed(_)));
}

#[tokio::test]
async fn run_rejects_absolute_path() {
    let tool = tool();
    let err = tool
        .run(serde_json::json!({
            "skill_name": "pdf-processing",
            "path": "/etc/passwd",
        }))
        .await
        .expect_err("absolute path must error");
    assert!(matches!(err, ToolError::ExecutionFailed(_)));
}

#[tokio::test]
async fn run_errors_on_missing_file() {
    let dir = std::env::temp_dir().join(format!("wisp-res-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut skill = skill_with_resources();
    skill.path = dir.clone();
    let tool = ReadSkillResourcesTool::new(vec![skill]);

    let err = tool
        .run(serde_json::json!({
            "skill_name": "pdf-processing",
            "path": "references/NOPE.md",
        }))
        .await
        .expect_err("missing file must error");

    let _ = std::fs::remove_dir_all(&dir);
    assert!(matches!(err, ToolError::ExecutionFailed(_)));
}

#[tokio::test]
async fn run_errors_on_unknown_skill() {
    let tool = tool();
    let err = tool
        .run(serde_json::json!({
            "skill_name": "nope",
            "path": "references/REFERENCE.md",
        }))
        .await
        .expect_err("unknown skill must error");
    assert!(matches!(err, ToolError::NotFound(_)));
}

#[tokio::test]
async fn run_errors_on_binary_file() {
    let dir = std::env::temp_dir().join(format!("wisp-res-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("image.png"), [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]).unwrap();
    let mut skill = skill_with_resources();
    skill.path = dir.clone();
    let tool = ReadSkillResourcesTool::new(vec![skill]);

    let err = tool
        .run(serde_json::json!({
            "skill_name": "pdf-processing",
            "path": "image.png",
        }))
        .await
        .expect_err("binary file must error");

    let _ = std::fs::remove_dir_all(&dir);
    assert!(matches!(err, ToolError::ExecutionFailed(_)));
}

#[tokio::test]
async fn run_errors_on_oversized_file() {
    let dir = std::env::temp_dir().join(format!("wisp-res-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let big = "x".repeat(600 * 1024); // > 512 KiB limit
    std::fs::write(dir.join("big.md"), big).unwrap();
    let mut skill = skill_with_resources();
    skill.path = dir.clone();
    let tool = ReadSkillResourcesTool::new(vec![skill]);

    let err = tool
        .run(serde_json::json!({
            "skill_name": "pdf-processing",
            "path": "big.md",
        }))
        .await
        .expect_err("oversized file must error");

    let _ = std::fs::remove_dir_all(&dir);
    assert!(matches!(err, ToolError::ExecutionFailed(_)));
}

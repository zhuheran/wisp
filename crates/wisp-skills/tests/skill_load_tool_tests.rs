//! SkillLoadTool（NativeTool 实现）测试（契约 1.6「SkillLoadTool」）。

use std::path::PathBuf;

use wisp_common::ToolContent;
use wisp_skills::{Skill, SkillLoadTool};
use wisp_software_tools::NativeTool;

fn sample_skill() -> Skill {
    Skill {
        name: "web-search".to_string(),
        description: "Search the web. Use when the user wants to find information online.".to_string(),
        license: Some("MIT".to_string()),
        compatibility: None,
        allowed_tools: Vec::new(),
        path: PathBuf::from("/tmp/skills/web-search"),
        body: "# Web Search\n\n1. Use the search engine.\n2. Summarize results.".to_string(),
    }
}

#[test]
fn tool_name_is_prefixed_with_skill() {
    let tool = SkillLoadTool::new(&sample_skill());
    assert_eq!(tool.name(), "skill:web-search");
}

#[test]
fn tool_description_is_verbatim() {
    let tool = SkillLoadTool::new(&sample_skill());
    assert_eq!(tool.description(), "Search the web. Use when the user wants to find information online.");
}

#[test]
fn tool_schema_has_no_parameters() {
    let tool = SkillLoadTool::new(&sample_skill());
    assert_eq!(tool.schema(), serde_json::json!({"type": "object", "properties": {}}));
}

#[tokio::test]
async fn tool_run_returns_body_text() {
    let skill = sample_skill();
    let tool = SkillLoadTool::new(&skill);
    let result = tool.run(serde_json::json!({})).await.expect("run should succeed");
    assert!(!result.is_error);
    assert_eq!(result.content, vec![ToolContent::Text { text: skill.body }]);
}

#[tokio::test]
async fn tool_run_ignores_arguments() {
    let skill = sample_skill();
    let tool = SkillLoadTool::new(&skill);
    let result = tool.run(serde_json::json!({"unexpected": "arg"})).await.expect("run should succeed");
    let text = match &result.content[0] {
        ToolContent::Text { text } => text,
        _ => panic!("expected text content"),
    };
    assert_eq!(text, &skill.body);
}

#[test]
fn tool_format_to_text_returns_body() {
    let skill = sample_skill();
    let tool = SkillLoadTool::new(&skill);
    let out = tool.format_to_text(tool.name(), &serde_json::json!({}), None);
    assert_eq!(out, skill.body);
}

#[test]
fn tool_format_to_markdown_returns_body() {
    let skill = sample_skill();
    let tool = SkillLoadTool::new(&skill);
    let out = tool.format_to_markdown(tool.name(), &serde_json::json!({}), None);
    assert_eq!(out, skill.body);
}

//! LoadSkillTool（NativeTool 实现）测试（契约 1.6「SkillLoadTool」→ 单一
//! `load_skill` 工具 + `skill_name` 参数设计）。

use std::path::PathBuf;

use wisp_common::{ToolContent, ToolError};
use wisp_skills::{LoadSkillTool, Skill};
use wisp_software_tools::NativeTool;

fn sample_skill(name: &str) -> Skill {
    Skill {
        name: name.to_string(),
        description: format!("Description of {name}."),
        license: None,
        compatibility: None,
        allowed_tools: Vec::new(),
        path: PathBuf::from("/tmp/skills").join(name),
        body: format!("# {name}\n\nInstructions body."),
    }
}

fn tool_with_two_skills() -> LoadSkillTool {
    LoadSkillTool::new(vec![sample_skill("web-search"), sample_skill("code-review")])
}

#[test]
fn tool_name_is_singleton_load_skill() {
    let tool = tool_with_two_skills();
    assert_eq!(tool.name(), "load_skill");
}

#[test]
fn tool_name_matches_openai_pattern() {
    let tool = tool_with_two_skills();
    let name = tool.name();
    assert!(
        !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "tool name {name:?} must match ^[a-zA-Z0-9_-]+$"
    );
}

#[test]
fn tool_description_points_to_system_prompt_list() {
    let tool = tool_with_two_skills();
    let desc = tool.description();
    assert!(desc.contains("skill_name"), "should mention the parameter");
    assert!(desc.contains("Available skills"), "should point at the L1 list");
}

#[test]
fn tool_schema_enumerates_skill_names() {
    let tool = tool_with_two_skills();
    let schema = tool.schema();
    let skill_name = &schema["properties"]["skill_name"];
    assert_eq!(skill_name["type"], "string");
    assert_eq!(
        skill_name["enum"],
        serde_json::json!(["web-search", "code-review"])
    );
    assert_eq!(schema["required"], serde_json::json!(["skill_name"]));
}

#[test]
fn tool_schema_enum_is_empty_for_no_skills() {
    let tool = LoadSkillTool::new(vec![]);
    let schema = tool.schema();
    assert_eq!(schema["properties"]["skill_name"]["enum"], serde_json::json!([]));
}

#[tokio::test]
async fn tool_run_returns_body_for_known_skill() {
    let tool = tool_with_two_skills();
    let result = tool
        .run(serde_json::json!({"skill_name": "code-review"}))
        .await
        .expect("run succeeds");
    assert!(!result.is_error);
    assert_eq!(
        result.content[0],
        ToolContent::Text { text: "# code-review\n\nInstructions body.".to_string() }
    );
}

#[tokio::test]
async fn tool_run_errors_on_unknown_skill() {
    let tool = tool_with_two_skills();
    let err = tool
        .run(serde_json::json!({"skill_name": "nope"}))
        .await
        .expect_err("unknown skill must error");
    assert!(matches!(err, ToolError::NotFound(_)));
}

#[tokio::test]
async fn tool_run_errors_on_missing_argument() {
    let tool = tool_with_two_skills();
    let err = tool
        .run(serde_json::json!({}))
        .await
        .expect_err("missing argument must error");
    assert!(matches!(err, ToolError::ExecutionFailed(_)));
}

#[test]
fn tool_format_returns_body_for_argument() {
    let tool = tool_with_two_skills();
    let args = serde_json::json!({"skill_name": "web-search"});
    let out = tool.format_to_text("load_skill", &args, None);
    assert_eq!(out, "# web-search\n\nInstructions body.");
}

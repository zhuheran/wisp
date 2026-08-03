//! frontmatter 解析测试（契约 1.6「解析」部分）。

use wisp_skills::{load_skill, SkillError};

mod common;

use common::{create_skill_dir, skill_md, TempDir};

fn load_ok(tmp: &TempDir, dir_name: &str) -> wisp_skills::Skill {
    load_skill(&tmp.path().join(dir_name)).expect("skill should load")
}

#[test]
fn parses_complete_frontmatter() {
    let tmp = TempDir::new();
    let content = skill_md(
        &[
            "name: web-search",
            "description: Search the web and summarize results",
            "license: MIT",
            "compatibility: 1.x",
            "allowed-tools: wisp_js_exec fetch",
        ],
        "# Web Search\n\nUse for searching.",
    );
    create_skill_dir(tmp.path(), "web-search", &content);

    let skill = load_ok(&tmp, "web-search");
    assert_eq!(skill.name, "web-search");
    assert_eq!(skill.description, "Search the web and summarize results");
    assert_eq!(skill.license.as_deref(), Some("MIT"));
    assert_eq!(skill.compatibility.as_deref(), Some("1.x"));
    assert_eq!(skill.allowed_tools, vec!["wisp_js_exec".to_string(), "fetch".to_string()]);
    assert_eq!(skill.path, tmp.path().join("web-search"));
    assert_eq!(skill.body, "# Web Search\n\nUse for searching.");
}

#[test]
fn values_are_trimmed() {
    let tmp = TempDir::new();
    let content = skill_md(&["name:   web-search   ", "description:   padded   "], "body");
    create_skill_dir(tmp.path(), "web-search", &content);

    let skill = load_ok(&tmp, "web-search");
    assert_eq!(skill.name, "web-search");
    assert_eq!(skill.description, "padded");
}

#[test]
fn missing_description_is_rejected() {
    let tmp = TempDir::new();
    let content = skill_md(&["name: web-search"], "body");
    create_skill_dir(tmp.path(), "web-search", &content);

    let err = load_skill(&tmp.path().join("web-search")).unwrap_err();
    assert!(matches!(err, SkillError::MissingDescription));
}

#[test]
fn description_longer_than_1024_is_rejected() {
    let tmp = TempDir::new();
    let long = "d".repeat(1025);
    let desc_line = format!("description: {long}");
    let lines = ["name: web-search", desc_line.as_str()];
    create_skill_dir(tmp.path(), "web-search", &skill_md(&lines, "body"));

    let err = load_skill(&tmp.path().join("web-search")).unwrap_err();
    assert!(matches!(err, SkillError::DescriptionTooLong(1025)));
}

#[test]
fn description_of_exactly_1024_is_accepted() {
    let tmp = TempDir::new();
    let long = "d".repeat(1024);
    let desc_line = format!("description: {long}");
    let lines = ["name: web-search", desc_line.as_str()];
    create_skill_dir(tmp.path(), "web-search", &skill_md(&lines, "body"));

    let skill = load_ok(&tmp, "web-search");
    assert_eq!(skill.description.len(), 1024);
}

#[test]
fn compatibility_longer_than_500_is_rejected() {
    let tmp = TempDir::new();
    let long = "c".repeat(501);
    let compat_line = format!("compatibility: {long}");
    let lines = ["name: web-search", "description: d", compat_line.as_str()];
    create_skill_dir(tmp.path(), "web-search", &skill_md(&lines, "body"));

    let err = load_skill(&tmp.path().join("web-search")).unwrap_err();
    assert!(matches!(err, SkillError::CompatibilityTooLong(501)));
}

#[test]
fn compatibility_of_exactly_500_is_accepted() {
    let tmp = TempDir::new();
    let long = "c".repeat(500);
    let compat_line = format!("compatibility: {long}");
    let lines = ["name: web-search", "description: d", compat_line.as_str()];
    create_skill_dir(tmp.path(), "web-search", &skill_md(&lines, "body"));

    let skill = load_ok(&tmp, "web-search");
    assert_eq!(skill.compatibility.as_deref().unwrap().len(), 500);
}

#[test]
fn no_frontmatter_is_rejected() {
    let tmp = TempDir::new();
    create_skill_dir(tmp.path(), "web-search", "plain text without frontmatter");

    let err = load_skill(&tmp.path().join("web-search")).unwrap_err();
    assert!(matches!(err, SkillError::MissingFrontmatter));
}

#[test]
fn missing_closing_delimiter_is_rejected() {
    let tmp = TempDir::new();
    create_skill_dir(tmp.path(), "web-search", "---\nname: web-search\ndescription: d");

    let err = load_skill(&tmp.path().join("web-search")).unwrap_err();
    assert!(matches!(err, SkillError::MissingFrontmatter));
}

#[test]
fn metadata_block_is_skipped() {
    let tmp = TempDir::new();
    let content = skill_md(
        &[
            "name: code-review",
            "description: Reviews pull requests",
            "metadata:",
            "  author: alice",
            "  version: 2.1",
        ],
        "Use this skill to review PRs.",
    );
    create_skill_dir(tmp.path(), "code-review", &content);

    let skill = load_ok(&tmp, "code-review");
    assert_eq!(skill.name, "code-review");
    assert_eq!(skill.description, "Reviews pull requests");
    assert_eq!(skill.body, "Use this skill to review PRs.");
}

#[test]
fn unknown_keys_are_ignored() {
    let tmp = TempDir::new();
    let content = skill_md(
        &["name: notes", "description: Keep notes", "x-custom: 42", "category: productivity"],
        "body",
    );
    create_skill_dir(tmp.path(), "notes", &content);

    let skill = load_ok(&tmp, "notes");
    assert_eq!(skill.name, "notes");
    assert_eq!(skill.description, "Keep notes");
}

#[test]
fn colon_in_value_is_preserved() {
    let tmp = TempDir::new();
    let content = skill_md(
        &["name: bookmarker", "description: See https://example.com/docs for usage; also http://a.b"],
        "body",
    );
    create_skill_dir(tmp.path(), "bookmarker", &content);

    let skill = load_ok(&tmp, "bookmarker");
    assert_eq!(skill.description, "See https://example.com/docs for usage; also http://a.b");
}

#[test]
fn allowed_tools_are_space_separated() {
    let tmp = TempDir::new();
    let content = skill_md(
        &["name: web-search", "description: d", "allowed-tools: wisp_js_exec fetch  grep"],
        "body",
    );
    create_skill_dir(tmp.path(), "web-search", &content);

    let skill = load_ok(&tmp, "web-search");
    assert_eq!(skill.allowed_tools, vec!["wisp_js_exec".to_string(), "fetch".to_string(), "grep".to_string()]);
}

#[test]
fn empty_allowed_tools_yields_empty_list() {
    let tmp = TempDir::new();
    let content = skill_md(&["name: web-search", "description: d", "allowed-tools:"], "body");
    create_skill_dir(tmp.path(), "web-search", &content);

    let skill = load_ok(&tmp, "web-search");
    assert!(skill.allowed_tools.is_empty());
}

#[test]
fn optional_fields_default_to_none() {
    let tmp = TempDir::new();
    let content = skill_md(&["name: minimal", "description: d"], "body");
    create_skill_dir(tmp.path(), "minimal", &content);

    let skill = load_ok(&tmp, "minimal");
    assert_eq!(skill.license, None);
    assert_eq!(skill.compatibility, None);
    assert!(skill.allowed_tools.is_empty());
}

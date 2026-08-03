//! assemble_skills_prompt 测试（契约 1.6「assemble_skills_prompt」）。

use std::path::PathBuf;

use wisp_skills::{assemble_skills_prompt, Skill};

fn make_skill(name: &str, description: &str) -> Skill {
    Skill {
        name: name.to_string(),
        description: description.to_string(),
        license: None,
        compatibility: None,
        allowed_tools: Vec::new(),
        path: PathBuf::from("/tmp").join(name),
        body: String::new(),
    }
}

#[test]
fn empty_list_yields_empty_string() {
    assert_eq!(assemble_skills_prompt(&[]), "");
}

#[test]
fn single_skill_lists_name_and_description() {
    let out = assemble_skills_prompt(&[make_skill("web-search", "Search the web")]);
    assert!(out.contains("Available skills:"));
    assert!(out.contains("- web-search: Search the web"));
}

#[test]
fn multiple_skills_are_formatted_with_name_and_description() {
    let skills = vec![
        make_skill("web-search", "Search the web"),
        make_skill("code-review", "Reviews code"),
    ];
    let out = assemble_skills_prompt(&skills);
    assert!(out.contains("Available skills:"));
    assert!(out.contains("- web-search: Search the web"));
    assert!(out.contains("- code-review: Reviews code"));
    assert!(out.lines().count() >= 3);
}

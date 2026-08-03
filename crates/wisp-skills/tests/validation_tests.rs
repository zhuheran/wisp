//! 校验测试（契约 1.6「校验」部分）：name 规则、目录名一致性、文件缺失。

use std::fs;

use wisp_skills::{load_skill, SkillError};

mod common;

use common::{create_skill_dir, skill_md, TempDir};

/// 在临时目录中创建 skill 并返回 load_skill 的错误。
fn err_for(dir_name: &str, frontmatter_lines: &[&str]) -> SkillError {
    let tmp = TempDir::new();
    create_skill_dir(tmp.path(), dir_name, &skill_md(frontmatter_lines, "body"));
    load_skill(&tmp.path().join(dir_name)).unwrap_err()
}

#[test]
fn name_with_uppercase_is_rejected() {
    let err = err_for("WebSearch", &["name: WebSearch", "description: d"]);
    assert!(matches!(err, SkillError::InvalidName(_)));
}

#[test]
fn name_with_illegal_characters_is_rejected() {
    for name in ["my_skill", "my skill", "web.search"] {
        let name_line = format!("name: {name}");
        let lines = [name_line.as_str(), "description: d"];
        let err = err_for(name, &lines);
        assert!(matches!(err, SkillError::InvalidName(_)), "name {name:?} should be rejected");
    }
}

#[test]
fn name_with_leading_hyphen_is_rejected() {
    let err = err_for("-lead", &["name: -lead", "description: d"]);
    assert!(matches!(err, SkillError::InvalidName(_)));
}

#[test]
fn name_with_trailing_hyphen_is_rejected() {
    let err = err_for("trail-", &["name: trail-", "description: d"]);
    assert!(matches!(err, SkillError::InvalidName(_)));
}

#[test]
fn name_with_double_hyphen_is_rejected() {
    let err = err_for("my--skill", &["name: my--skill", "description: d"]);
    assert!(matches!(err, SkillError::InvalidName(_)));
}

#[test]
fn name_longer_than_64_is_rejected() {
    let name = "a".repeat(65);
    let name_line = format!("name: {name}");
    let lines = [name_line.as_str(), "description: d"];
    let err = err_for(&name, &lines);
    assert!(matches!(err, SkillError::InvalidName(_)));
}

#[test]
fn name_of_exactly_64_is_accepted() {
    let name = "a".repeat(64);
    let tmp = TempDir::new();
    let name_line = format!("name: {name}");
    let lines = [name_line.as_str(), "description: d"];
    create_skill_dir(tmp.path(), &name, &skill_md(&lines, "body"));

    let skill = load_skill(&tmp.path().join(&name)).expect("64-char name should load");
    assert_eq!(skill.name, name);
}

#[test]
fn empty_name_is_rejected() {
    let err = err_for("web-search", &["name:", "description: d"]);
    assert!(matches!(err, SkillError::InvalidName(name) if name.is_empty()));
}

#[test]
fn name_mismatching_dir_is_rejected() {
    let tmp = TempDir::new();
    let content = skill_md(&["name: beta", "description: d"], "body");
    create_skill_dir(tmp.path(), "alpha", &content);

    let err = load_skill(&tmp.path().join("alpha")).unwrap_err();
    match err {
        SkillError::NameMismatch { dir, name } => {
            assert_eq!(dir, "alpha");
            assert_eq!(name, "beta");
        },
        other => panic!("expected NameMismatch, got {other:?}"),
    }
}

#[test]
fn missing_skill_md_is_rejected() {
    let tmp = TempDir::new();
    let dir = tmp.path().join("empty-dir");
    fs::create_dir_all(&dir).unwrap();

    let err = load_skill(&dir).unwrap_err();
    match err {
        SkillError::NoSkillMd(path) => assert_eq!(path, dir),
        other => panic!("expected NoSkillMd, got {other:?}"),
    }
}

#[test]
fn nonexistent_dir_returns_io_error() {
    let tmp = TempDir::new();
    let err = load_skill(&tmp.path().join("nope")).unwrap_err();
    assert!(matches!(err, SkillError::Io(_)));
}

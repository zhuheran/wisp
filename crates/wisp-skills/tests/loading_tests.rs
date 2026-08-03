//! load_skills 扫描测试（契约 1.6「load_skills 混合目录宽容模式」）。

use std::fs;

use wisp_skills::{load_skills, SkillError};

mod common;

use common::{create_skill_dir, skill_md, TempDir};

#[test]
fn mixed_directory_is_lenient() {
    let tmp = TempDir::new();
    // 合法
    create_skill_dir(tmp.path(), "good", &skill_md(&["name: good", "description: d"], "body"));
    // 非法 name（大写）
    create_skill_dir(tmp.path(), "BadName", &skill_md(&["name: BadName", "description: d"], "body"));
    // 有 SKILL.md 但无 frontmatter
    create_skill_dir(tmp.path(), "no-fm", "no frontmatter here");
    // 目录里没有 SKILL.md
    fs::create_dir_all(tmp.path().join("empty-dir")).unwrap();

    let (skills, errors) = load_skills(tmp.path());

    assert_eq!(skills.len(), 1, "only the valid skill should load");
    assert_eq!(skills[0].name, "good");

    assert_eq!(errors.len(), 3, "all bad directories should be reported");
    let mut error_names: Vec<&str> = errors.iter().map(|(n, _)| n.as_str()).collect();
    error_names.sort_unstable();
    assert_eq!(error_names, vec!["BadName", "empty-dir", "no-fm"]);

    let bad_name = errors.iter().find(|(n, _)| n == "BadName").unwrap();
    assert!(matches!(bad_name.1, SkillError::InvalidName(_)));
    let no_fm = errors.iter().find(|(n, _)| n == "no-fm").unwrap();
    assert!(matches!(no_fm.1, SkillError::MissingFrontmatter));
    let empty = errors.iter().find(|(n, _)| n == "empty-dir").unwrap();
    assert!(matches!(empty.1, SkillError::NoSkillMd(_)));
}

#[test]
fn empty_directory_yields_nothing() {
    let tmp = TempDir::new();
    let (skills, errors) = load_skills(tmp.path());
    assert!(skills.is_empty());
    assert!(errors.is_empty());
}

#[test]
fn stray_files_are_ignored() {
    let tmp = TempDir::new();
    fs::write(tmp.path().join("README.md"), "hi").unwrap();
    create_skill_dir(tmp.path(), "good", &skill_md(&["name: good", "description: d"], "body"));

    let (skills, errors) = load_skills(tmp.path());
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name, "good");
    assert!(errors.is_empty());
}

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

#[test]
fn resources_are_scanned_recursively_excluding_skill_md() {
    let tmp = TempDir::new();
    let dir = tmp.path().join("good");
    create_skill_dir(tmp.path(), "good", &skill_md(&["name: good", "description: d"], "body"));
    fs::create_dir_all(dir.join("references")).unwrap();
    fs::write(dir.join("references/REFERENCE.md"), "ref").unwrap();
    fs::create_dir_all(dir.join("scripts")).unwrap();
    fs::write(dir.join("scripts/extract.py"), "print(1)").unwrap();
    fs::write(dir.join("notes.md"), "note").unwrap();

    let (skills, _) = load_skills(tmp.path());
    let skill = &skills[0];
    assert_eq!(
        skill.resources,
        vec![
            "notes.md".to_string(),
            "references/REFERENCE.md".to_string(),
            "scripts/extract.py".to_string(),
        ],
        "resources should be sorted, recursive, and exclude SKILL.md"
    );
}

#[test]
fn skill_without_resources_has_empty_list() {
    let tmp = TempDir::new();
    create_skill_dir(tmp.path(), "good", &skill_md(&["name: good", "description: d"], "body"));

    let (skills, _) = load_skills(tmp.path());
    assert!(skills[0].resources.is_empty());
}

#[test]
fn resources_filter_out_hidden_and_cache_files() {
    let tmp = TempDir::new();
    let dir = tmp.path().join("good");
    create_skill_dir(tmp.path(), "good", &skill_md(&["name: good", "description: d"], "body"));
    fs::create_dir_all(dir.join("references")).unwrap();
    fs::write(dir.join("references/REFERENCE.md"), "ref").unwrap();
    fs::write(dir.join("notes.md"), "note").unwrap();
    fs::write(dir.join(".hidden.md"), "hidden").unwrap();
    fs::write(dir.join(".DS_Store"), "junk").unwrap();
    fs::create_dir_all(dir.join("__pycache__")).unwrap();
    fs::write(dir.join("__pycache__/x.pyc"), "junk").unwrap();

    let (skills, _) = load_skills(tmp.path());
    assert_eq!(
        skills[0].resources,
        vec![
            "notes.md".to_string(),
            "references/REFERENCE.md".to_string(),
        ],
        "hidden and cache files must be excluded"
    );
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use wisp_common::{ToolError, ToolResult};
use wisp_skills::{load_skills, SkillError, SkillLoadTool};
use wisp_software_tools::NativeTool;
use wisp_tool_registry::{ToolDefinition, ToolHandler, ToolRegistry};

use crate::types::AppData;

#[derive(Debug, Clone, Serialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub path: String,
}

/// Resolve the skills directories to scan, in priority order:
/// 1. `app_data_dir()/skills` — app-owned, created on demand, writable
/// 2. `~/.agents/skills` — the global agent-skills directory shared with
///    other tools (Claude Code / Zed); scanned read-only if it exists.
pub(crate) fn skills_dirs(app_handle: &AppHandle) -> Result<Vec<PathBuf>, String> {
    let base = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {e}"))?;
    let app_dir = base.join("skills");
    std::fs::create_dir_all(&app_dir).map_err(|e| format!("Failed to create skills dir: {e}"))?;

    let mut dirs = vec![app_dir];
    if let Some(home) = home_dir() {
        let global = home.join(".agents").join("skills");
        if global.is_dir() {
            dirs.push(global);
        }
    }
    Ok(dirs)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

/// Scan all skill directories in priority order. When the same skill name
/// exists in multiple directories, the first (higher-priority) one wins.
pub(crate) fn load_skills_from_dirs(
    dirs: &[PathBuf],
) -> (Vec<wisp_skills::Skill>, Vec<(String, SkillError)>) {
    let mut skills: Vec<wisp_skills::Skill> = Vec::new();
    let mut errors: Vec<(String, SkillError)> = Vec::new();
    for dir in dirs {
        let (found, dir_errors) = load_skills(dir);
        errors.extend(dir_errors);
        for skill in found {
            if !skills.iter().any(|s| s.name == skill.name) {
                skills.push(skill);
            }
        }
    }
    (skills, errors)
}

/// `NativeTool` adapter used to register `SkillLoadTool`s into the shared
/// `ToolRegistry` (mirrors `wisp_software_tools::NativeToolAdapter`).
struct SkillToolHandler {
    tool: SkillLoadTool,
}

#[async_trait]
impl ToolHandler for SkillToolHandler {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        self.tool.run(args).await
    }
}

fn build_definition(tool: &SkillLoadTool) -> ToolDefinition {
    ToolDefinition {
        name: tool.name().to_string(),
        description: Some(tool.description().to_string()),
        input_schema: tool.schema(),
        annotations: None,
        metadata: HashMap::from([(
            "provider".to_string(),
            Value::String("native".to_string()),
        )]),
        requires_confirmation: false,
    }
}

/// Register every loaded skill into the tool registry. `ToolRegistry::register`
/// preserves the enabled state of already-registered tools (so a refresh keeps
/// the user's toggles); tools whose skill directories disappeared are
/// unregistered; newly scanned skills default to enabled.
pub(crate) fn resync_registry(registry: &ToolRegistry, skills: &[wisp_skills::Skill]) {
    let existing: Vec<String> = registry
        .list_tools()
        .into_iter()
        .filter(|t| t.name.starts_with("skill:"))
        .map(|t| t.name)
        .collect();
    let current: std::collections::HashSet<String> =
        skills.iter().map(|s| format!("skill:{}", s.name)).collect();

    for name in existing {
        if !current.contains(&name) {
            registry.unregister(&name);
        }
    }
    for skill in skills {
        let tool = SkillLoadTool::new(skill);
        let definition = build_definition(&tool);
        let handler = Arc::new(SkillToolHandler { tool }) as Arc<dyn ToolHandler>;
        registry.register(definition, handler, Vec::new());
    }
}

fn collect_skill_infos(skills: &[wisp_skills::Skill], registry: &ToolRegistry) -> Vec<SkillInfo> {
    let enabled_set = registry.enabled_set();
    let mut infos: Vec<SkillInfo> = skills
        .iter()
        .map(|s| SkillInfo {
            name: s.name.clone(),
            description: s.description.clone(),
            enabled: enabled_set.contains(&format!("skill:{}", s.name)),
            path: s.path.display().to_string(),
        })
        .collect();
    infos.sort_by(|a, b| a.name.cmp(&b.name));
    infos
}

/// List installed skills with their enabled state.
#[tauri::command]
pub async fn skills_list(app_handle: AppHandle) -> Result<Vec<SkillInfo>, String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(collect_skill_infos(&state.skills, &state.tool_registry))
}

/// Re-scan the skills directory, update the registered tools, and return the
/// new list. Scan errors are reported per-skill through the returned list's
/// absence; the directory itself is created if missing.
#[tauri::command]
pub async fn skills_refresh(app_handle: AppHandle) -> Result<Vec<SkillInfo>, String> {
    let dirs = skills_dirs(&app_handle)?;
    let (skills, _errors) = load_skills_from_dirs(&dirs);

    let state_mutex = app_handle.state::<Mutex<AppData>>();
    let mut state = state_mutex.lock().map_err(|e| e.to_string())?;
    resync_registry(&state.tool_registry, &skills);
    state.skills = skills;
    Ok(collect_skill_infos(&state.skills, &state.tool_registry))
}

/// Toggle a skill's enabled state (enabled skills are injected into the
/// system prompt as L1 metadata and registered as callable tools).
#[tauri::command]
pub async fn skills_toggle(app_handle: AppHandle, name: String) -> Result<Vec<SkillInfo>, String> {
    let tool_name = format!("skill:{name}");
    let state_mutex = app_handle.state::<Mutex<AppData>>();
    let state = state_mutex.lock().map_err(|e| e.to_string())?;

    if !state.skills.iter().any(|s| s.name == name) {
        return Err(format!("Skill not found: {name}"));
    }

    let currently_enabled = state.tool_registry.enabled_set().contains(&tool_name);
    state.tool_registry.set_tool_enabled(&tool_name, !currently_enabled);
    Ok(collect_skill_infos(&state.skills, &state.tool_registry))
}

/// Open the app-owned skills directory in the system file manager (the
/// writable location where users drop new SKILL.md directories).
#[tauri::command]
pub async fn skills_open_folder(app_handle: AppHandle) -> Result<(), String> {
    let app_dir = skills_dirs(&app_handle)?
        .into_iter()
        .next()
        .ok_or_else(|| "No skills directory available".to_string())?;
    app_handle
        .opener()
        .open_path(app_dir.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|e| format!("Failed to open skills folder: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn skill(name: &str, description: &str) -> wisp_skills::Skill {
        wisp_skills::Skill {
            name: name.to_string(),
            description: description.to_string(),
            license: None,
            compatibility: None,
            allowed_tools: vec![],
            path: PathBuf::from("/tmp/skills").join(name),
            body: "# body".to_string(),
        }
    }

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!("wisp-skills-test-{}", uuid::Uuid::new_v4()))
    }

    fn write_skill_dir(base: &std::path::Path, name: &str, description: &str) {
        let dir = base.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n\n# body\n"),
        )
        .unwrap();
    }

    #[test]
    fn load_from_dirs_merges_and_dedupes_by_priority() {
        let root = temp_root();
        let app_dir = root.join("app");
        let global_dir = root.join("global");
        write_skill_dir(&app_dir, "alpha", "Alpha skill");
        write_skill_dir(&app_dir, "beta", "Beta from app");
        write_skill_dir(&global_dir, "beta", "Beta from global");
        write_skill_dir(&global_dir, "gamma", "Gamma skill");

        let (skills, errors) = load_skills_from_dirs(&[app_dir, global_dir]);
        let _ = fs::remove_dir_all(&root);

        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        // Order is filesystem-dependent; assert presence, dedup, and priority.
        let names: std::collections::HashSet<String> =
            skills.iter().map(|s| s.name.clone()).collect();
        assert_eq!(names.len(), 3);
        assert!(names.contains("alpha"));
        assert!(names.contains("beta"));
        assert!(names.contains("gamma"));
        // Same name in both dirs: the first (higher-priority) dir wins.
        let beta = skills.iter().find(|s| s.name == "beta").unwrap();
        assert_eq!(beta.description, "Beta from app");
    }

    #[test]
    fn load_from_dirs_reports_bad_skills_and_skips_missing_dirs() {
        let root = temp_root();
        let app_dir = root.join("app");
        write_skill_dir(&app_dir, "ok", "OK skill");
        let bad = app_dir.join("bad");
        fs::create_dir_all(&bad).unwrap();
        fs::write(bad.join("SKILL.md"), "no frontmatter here").unwrap();

        let (skills, errors) = load_skills_from_dirs(&[root.join("nonexistent"), app_dir]);
        let _ = fs::remove_dir_all(&root);

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "ok");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, "bad");
    }

    #[test]
    fn resync_registers_new_skills_as_enabled() {
        let registry = ToolRegistry::new();
        let skills = vec![skill("alpha", "A"), skill("beta", "B")];

        resync_registry(&registry, &skills);

        assert!(registry.get_tool("skill:alpha").is_some());
        assert!(registry.get_tool("skill:beta").is_some());
        assert!(registry.enabled_set().contains("skill:alpha"));
        assert!(registry.enabled_set().contains("skill:beta"));
    }

    #[test]
    fn resync_preserves_enabled_state_across_refresh() {
        let registry = ToolRegistry::new();
        resync_registry(&registry, &[skill("alpha", "A")]);
        registry.set_tool_enabled("skill:alpha", false);

        // Same skill rescanned: enabled state must survive.
        resync_registry(&registry, &[skill("alpha", "A updated")]);

        assert!(!registry.enabled_set().contains("skill:alpha"));
        // Definition updated with the new description.
        let tool = registry.get_tool("skill:alpha").expect("tool");
        assert_eq!(tool.description.as_deref(), Some("A updated"));
    }

    #[test]
    fn resync_unregisters_removed_skills() {
        let registry = ToolRegistry::new();
        resync_registry(&registry, &[skill("alpha", "A"), skill("beta", "B")]);

        // beta's directory disappeared.
        resync_registry(&registry, &[skill("alpha", "A")]);

        assert!(registry.get_tool("skill:alpha").is_some());
        assert!(registry.get_tool("skill:beta").is_none());
    }

    #[test]
    fn collect_infos_sorts_by_name_and_marks_enabled() {
        let registry = ToolRegistry::new();
        let skills = vec![skill("beta", "B"), skill("alpha", "A")];
        resync_registry(&registry, &skills);
        registry.set_tool_enabled("skill:beta", false);

        let infos = collect_skill_infos(&skills, &registry);

        assert_eq!(infos.len(), 2);
        assert_eq!(infos[0].name, "alpha");
        assert!(infos[0].enabled);
        assert_eq!(infos[1].name, "beta");
        assert!(!infos[1].enabled);
        assert_eq!(infos[0].description, "A");
        assert!(infos[0].path.ends_with("alpha"));
    }

    /// Manual smoke test against the real `~/.agents/skills` directory.
    /// Run with: cargo test -p wisp smoke_real_global_skills -- --ignored --nocapture
    #[test]
    #[ignore = "depends on the developer machine's ~/.agents/skills directory"]
    fn smoke_real_global_skills() {
        let home = home_dir().expect("home dir");
        let dir = home.join(".agents").join("skills");
        assert!(dir.is_dir(), "{dir:?} is not a directory");

        let (skills, errors) = load_skills(&dir);
        println!("loaded {} skills, {} errors", skills.len(), errors.len());
        for (name, err) in &errors {
            println!("  FAIL {name}: {err}");
        }
        assert!(!skills.is_empty(), "no skills loaded from {dir:?}");
    }
}

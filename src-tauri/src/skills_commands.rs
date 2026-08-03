use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use wisp_common::{ToolError, ToolResult};
use wisp_skills::{load_skills, LoadSkillTool, Skill, SkillError};
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
) -> (Vec<Skill>, Vec<(String, SkillError)>) {
    let mut skills: Vec<Skill> = Vec::new();
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

/// `NativeTool` adapter used to register `LoadSkillTool` into the shared
/// `ToolRegistry` (mirrors `wisp_software_tools::NativeToolAdapter`).
struct SkillToolHandler {
    tool: LoadSkillTool,
}

#[async_trait]
impl ToolHandler for SkillToolHandler {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        self.tool.run(args).await
    }
}

fn build_definition(tool: &LoadSkillTool) -> ToolDefinition {
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

/// (Re-)register the single `load_skill` tool for the enabled skills.
///
/// Only enabled skills are exposed via the tool's `skill_name` enum; the
/// per-skill enabled set lives in `AppData.enabled_skills`. Stale tools are
/// unregistered: the current `load_skill` entry (re-registered with the new
/// schema) and any `skill_*` tools left over from the earlier one-tool-per-
/// skill design.
pub(crate) fn resync_registry(registry: &ToolRegistry, skills: &[Skill], enabled: &HashSet<String>) {
    for name in registry
        .list_tools()
        .into_iter()
        .map(|t| t.name)
        .filter(|n| n == LoadSkillTool::TOOL_NAME || n.starts_with("skill_"))
        .collect::<Vec<_>>()
    {
        registry.unregister(&name);
    }

    let enabled_skills: Vec<Skill> = skills
        .iter()
        .filter(|s| enabled.contains(&s.name))
        .cloned()
        .collect();
    if enabled_skills.is_empty() {
        return;
    }

    let tool = LoadSkillTool::new(enabled_skills);
    let definition = build_definition(&tool);
    let handler = Arc::new(SkillToolHandler { tool }) as Arc<dyn ToolHandler>;
    registry.register(definition, handler, Vec::new());
}

fn collect_skill_infos(skills: &[Skill], enabled: &HashSet<String>) -> Vec<SkillInfo> {
    let mut infos: Vec<SkillInfo> = skills
        .iter()
        .map(|s| SkillInfo {
            name: s.name.clone(),
            description: s.description.clone(),
            enabled: enabled.contains(&s.name),
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
    Ok(collect_skill_infos(&state.skills, &state.enabled_skills))
}

/// Re-scan the skills directories, update the registered tool, and return the
/// new list. Skills that were enabled stay enabled; newly discovered skills
/// default to enabled; removed skills drop out.
#[tauri::command]
pub async fn skills_refresh(app_handle: AppHandle) -> Result<Vec<SkillInfo>, String> {
    let dirs = skills_dirs(&app_handle)?;
    let (skills, _errors) = load_skills_from_dirs(&dirs);

    let state_mutex = app_handle.state::<Mutex<AppData>>();
    let mut state = state_mutex.lock().map_err(|e| e.to_string())?;

    let found: HashSet<String> = skills.iter().map(|s| s.name.clone()).collect();
    let mut enabled = std::mem::take(&mut state.enabled_skills);
    enabled.extend(found);

    resync_registry(&state.tool_registry, &skills, &enabled);
    state.skills = skills;
    state.enabled_skills = enabled;
    Ok(collect_skill_infos(&state.skills, &state.enabled_skills))
}

/// Toggle a skill's enabled state (enabled skills are advertised in the
/// system prompt as L1 metadata and exposed via the `load_skill` tool).
#[tauri::command]
pub async fn skills_toggle(app_handle: AppHandle, name: String) -> Result<Vec<SkillInfo>, String> {
    let state_mutex = app_handle.state::<Mutex<AppData>>();
    let mut state = state_mutex.lock().map_err(|e| e.to_string())?;

    if !state.skills.iter().any(|s| s.name == name) {
        return Err(format!("Skill not found: {name}"));
    }

    if !state.enabled_skills.remove(&name) {
        state.enabled_skills.insert(name);
    }
    resync_registry(&state.tool_registry, &state.skills, &state.enabled_skills);
    Ok(collect_skill_infos(&state.skills, &state.enabled_skills))
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

    fn skill(name: &str, description: &str) -> Skill {
        Skill {
            name: name.to_string(),
            description: description.to_string(),
            license: None,
            compatibility: None,
            allowed_tools: vec![],
            path: PathBuf::from("/tmp/skills").join(name),
            body: "# body".to_string(),
        }
    }

    fn enabled(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
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
        let names: HashSet<String> = skills.iter().map(|s| s.name.clone()).collect();
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
    fn resync_registers_single_tool_with_enabled_enum() {
        let registry = ToolRegistry::new();
        let skills = vec![skill("alpha", "A"), skill("beta", "B")];

        resync_registry(&registry, &skills, &enabled(&["alpha"]));

        // Exactly one tool, containing only enabled skills.
        let tools = registry.list_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "load_skill");
        let schema = &tools[0].input_schema;
        assert_eq!(schema["properties"]["skill_name"]["enum"], serde_json::json!(["alpha"]));
    }

    #[test]
    fn resync_with_no_enabled_skills_registers_nothing() {
        let registry = ToolRegistry::new();
        let skills = vec![skill("alpha", "A")];

        resync_registry(&registry, &skills, &enabled(&[]));

        assert!(registry.list_tools().is_empty());
    }

    #[test]
    fn resync_updates_enum_and_cleans_stale_tools() {
        let registry = ToolRegistry::new();
        let skills = vec![skill("alpha", "A"), skill("beta", "B")];

        // First scan: both enabled.
        resync_registry(&registry, &skills, &enabled(&["alpha", "beta"]));
        // Legacy one-tool-per-skill entry from the previous design.
        registry.register(
            ToolDefinition {
                name: "skill_alpha".to_string(),
                description: Some("stale".to_string()),
                input_schema: serde_json::json!({"type": "object"}),
                annotations: None,
                metadata: HashMap::new(),
                requires_confirmation: false,
            },
            Arc::new(SkillToolHandler { tool: LoadSkillTool::new(vec![]) }),
            Vec::new(),
        );

        // Second scan: alpha disabled, beta updated.
        let skills = vec![skill("alpha", "A updated"), skill("beta", "B")];
        resync_registry(&registry, &skills, &enabled(&["beta"]));

        let tools = registry.list_tools();
        assert_eq!(tools.len(), 1, "stale skill_alpha must be removed");
        assert_eq!(tools[0].name, "load_skill");
        let schema = &tools[0].input_schema;
        assert_eq!(schema["properties"]["skill_name"]["enum"], serde_json::json!(["beta"]));
    }

    #[test]
    fn collect_infos_sorts_by_name_and_marks_enabled() {
        let skills = vec![skill("beta", "B"), skill("alpha", "A")];

        let infos = collect_skill_infos(&skills, &enabled(&["alpha"]));

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

//! 测试共享辅助：唯一临时目录（`std::env::temp_dir()` + `uuid::Uuid::new_v4()`，
//! 参照契约 1.6；`tempfile` 不在 workspace 依赖中，故手写并随 Drop 清理）。

use std::fs;
use std::path::{Path, PathBuf};

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wisp-skills-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create temp dir");
        TempDir(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Default for TempDir {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 在 `root` 下创建名为 `dir_name` 的子目录并写入 SKILL.md，返回该目录路径。
pub fn create_skill_dir(root: &Path, dir_name: &str, skill_md_content: &str) -> PathBuf {
    let dir = root.join(dir_name);
    fs::create_dir_all(&dir).expect("create skill dir");
    fs::write(dir.join("SKILL.md"), skill_md_content).expect("write SKILL.md");
    dir
}

/// 由 frontmatter 行 + 正文拼装 SKILL.md 内容。
pub fn skill_md(frontmatter_lines: &[&str], body: &str) -> String {
    format!("---\n{}\n---\n{}", frontmatter_lines.join("\n"), body)
}

use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Default)]
pub struct ProjectDocs {
    pub instructions: Vec<PathBuf>, // ordered root → deepest
    pub readmes: Vec<PathBuf>,
    pub docs_dirs: Vec<PathBuf>,
}

const INSTRUCTION_FILES: &[&str] = &["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CONTRIBUTING.md"];

pub fn discover(root: &Path) -> ProjectDocs {
    let mut docs = ProjectDocs::default();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy();
            name != ".git" && name != "node_modules" && name != "target" && name != ".ctx"
        })
    {
        let path = entry.path();
        if path.is_file() {
            let name = path.file_name().unwrap().to_string_lossy();
            if INSTRUCTION_FILES.contains(&name.as_ref()) {
                docs.instructions.push(path.to_path_buf());
            } else if name.eq_ignore_ascii_case("README.md") {
                docs.readmes.push(path.to_path_buf());
            }
        } else if path.is_dir() && path.file_name().map(|n| n == "docs").unwrap_or(false) {
            docs.docs_dirs.push(path.to_path_buf());
        }
    }
    docs.instructions.sort();
    docs.readmes.sort();
    docs.docs_dirs.sort();
    docs
}

/// Merge applicable instruction files for a working directory, root first.
pub fn instructions_for(root: &Path, cwd: &Path) -> Vec<PathBuf> {
    let mut docs = discover(root);
    docs.instructions
        .retain(|p| cwd.starts_with(p.parent().unwrap_or(root)));
    docs.instructions
}

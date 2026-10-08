use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Default)]
pub struct ProjectDocs {
    pub instructions: Vec<PathBuf>, // ordered shallow → deep
    pub readmes: Vec<PathBuf>,
    pub docs_dirs: Vec<PathBuf>,
}

const INSTRUCTION_FILES: &[&str] = &["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CONTRIBUTING.md"];
const PRUNE_DIRS: &[&str] = &[".git", "node_modules", "target", ".ctx", "dist", "build"];

pub fn discover(root: &Path) -> ProjectDocs {
    let mut docs = ProjectDocs::default();
    let mut walker = WalkDir::new(root).follow_links(false).into_iter();
    loop {
        let entry = match walker.next() {
            Some(Ok(e)) => e,
            Some(Err(_)) => continue,
            None => break,
        };
        let name = entry.file_name().to_string_lossy();
        if entry.file_type().is_dir() && PRUNE_DIRS.contains(&name.as_ref()) {
            walker.skip_current_dir();
            continue;
        }
        let path = entry.path();
        if path.is_file() {
            if INSTRUCTION_FILES.contains(&name.as_ref()) {
                docs.instructions.push(path.to_path_buf());
            } else if name.eq_ignore_ascii_case("README.md") {
                docs.readmes.push(path.to_path_buf());
            }
        } else if path.is_dir() && name == "docs" {
            docs.docs_dirs.push(path.to_path_buf());
        }
    }
    let by_depth = |a: &PathBuf, b: &PathBuf| {
        a.components()
            .count()
            .cmp(&b.components().count())
            .then(a.cmp(b))
    };
    docs.instructions.sort_by(by_depth);
    docs.readmes.sort_by(by_depth);
    docs.docs_dirs.sort_by(by_depth);
    docs
}

/// All instruction files applicable to a working directory, shallow → deep.
/// An instruction applies when its directory is the cwd or an ancestor of it.
pub fn instructions_for(root: &Path, cwd: &Path) -> Vec<PathBuf> {
    let mut docs = discover(root);
    docs.instructions
        .retain(|p| cwd.starts_with(p.parent().unwrap_or(root)));
    docs.instructions
}

/// Same, but the instruction content concatenated with a header per file.
pub fn merged_instructions(root: &Path, cwd: &Path) -> String {
    let mut out = String::new();
    for path in instructions_for(root, cwd) {
        let rel = path.strip_prefix(root).unwrap_or(&path);
        out.push_str(&format!("<!-- {} -->\n", rel.display()));
        if let Ok(content) = std::fs::read_to_string(&path) {
            out.push_str(&content);
            if !content.ends_with('\n') {
                out.push('\n');
            }
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovers_and_merges_hierarchy() {
        let dir = std::env::temp_dir().join(format!("ctx-docs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("packages/api")).unwrap();
        std::fs::write(dir.join("AGENTS.md"), "root rules").unwrap();
        std::fs::write(dir.join("packages/api/AGENTS.md"), "api rules").unwrap();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join(".git/AGENTS.md"), "should be pruned").unwrap();

        let docs = discover(&dir);
        assert_eq!(docs.instructions.len(), 2);

        let api = dir.join("packages/api");
        let merged = merged_instructions(&dir, &api);
        assert!(merged.contains("root rules"));
        assert!(merged.contains("api rules"));
        assert!(merged.find("root rules").unwrap() < merged.find("api rules").unwrap());

        let rootview = merged_instructions(&dir, &dir);
        assert!(rootview.contains("root rules"));
        assert!(!rootview.contains("api rules"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

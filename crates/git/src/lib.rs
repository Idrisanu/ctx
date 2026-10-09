use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct GitInfo {
    pub is_repo: bool,
    pub branch: Option<String>,
    pub last_commit: Option<String>,
    pub dirty_files: Vec<String>,
    pub uncommitted_count: usize,
}

fn run_git(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .context("failed to run git")?;
    if !out.status.success() {
        anyhow::bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn run_git_raw(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .context("failed to run git")?;
    if !out.status.success() {
        anyhow::bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn info(root: &Path) -> GitInfo {
    let mut info = GitInfo::default();
    let Ok(top) = run_git(root, &["rev-parse", "--is-inside-work-tree"]) else {
        return info;
    };
    if top != "true" {
        return info;
    }
    info.is_repo = true;
    info.branch = run_git(root, &["rev-parse", "--abbrev-ref", "HEAD"]).ok();
    info.last_commit = run_git(root, &["rev-parse", "--short", "HEAD"]).ok();
    let raw_porcelain = run_git_raw(root, &["status", "--porcelain"]);
    if let Ok(porcelain) = raw_porcelain {
        let files: Vec<String> = porcelain
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l[3..].trim().to_string())
            .collect();
        info.uncommitted_count = files.len();
        info.dirty_files = files;
    }
    info
}

/// Patterns from `<root>/.ctxignore` (gitignore-ish, small subset):
/// blank lines and `#` comments skipped; `dir/` matches a prefix;
/// `*.ext` matches file extension; otherwise exact relative path.
pub fn ctxignore_patterns(root: &Path) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(root.join(".ctxignore")) else {
        return Vec::new();
    };
    content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

pub fn ctxignored(patterns: &[String], rel_path: &str) -> bool {
    let p = rel_path.trim_start_matches("./");
    patterns.iter().any(|pat| {
        if let Some(dir) = pat.strip_suffix('/') {
            p == dir || p.starts_with(&format!("{}/", dir))
        } else if let Some(ext) = pat.strip_prefix("*.") {
            p.ends_with(&format!(".{}", ext)) || p == ext
        } else {
            p == pat
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ctxignore_matching() {
        let pats = vec![
            ".env".to_string(),
            "*.pem".to_string(),
            "secrets/".to_string(),
        ];
        assert!(ctxignored(&pats, ".env"));
        assert!(ctxignored(&pats, "certs/key.pem"));
        assert!(ctxignored(&pats, "secrets/token.txt"));
        assert!(!ctxignored(&pats, "src/main.rs"));
        assert!(!ctxignored(&pats, ".env.example"));
    }
}

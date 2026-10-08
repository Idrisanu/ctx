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
    if let Ok(porcelain) = run_git(root, &["status", "--porcelain"]) {
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

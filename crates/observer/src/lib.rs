use anyhow::{Context, Result};
use ctx_core::{EnvironmentState, ProjectContext};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Duration;

/// Watch the project tree and keep `state.files_changed` in sync with the
/// set of files that have changed since the last checkpoint marker.
pub fn watch(root: PathBuf, ctx_dir: PathBuf) -> Result<()> {
    use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};

    let (tx, rx) = channel();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    watcher.watch(&root, RecursiveMode::Recursive)?;

    println!("Watching {} — Ctrl-C to stop", root.display());
    let mut state = load_state(&ctx_dir).unwrap_or_default();
    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(event)) => {
                let mut changed = false;
                for path in event.paths {
                    if should_ignore(&path, &root) {
                        continue;
                    }
                    let rel = path
                        .strip_prefix(&root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();
                    if !state.files_changed.contains(&rel) {
                        state.files_changed.push(rel);
                        // keep bounded
                        if state.files_changed.len() > 500 {
                            state.files_changed.remove(0);
                        }
                        changed = true;
                    }
                }
                if changed {
                    let _ = write_state(&ctx_dir, &state);
                }
            }
            Ok(Err(e)) => eprintln!("watch error: {e}"),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}

fn should_ignore(path: &Path, root: &Path) -> bool {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let s = rel.to_string_lossy();
    s.starts_with(".git")
        || s.starts_with(".ctx")
        || s.contains("node_modules")
        || s.contains("/target/")
        || s == "target"
}

fn load_state(ctx_dir: &Path) -> Option<ProjectContext> {
    let s = std::fs::read_to_string(ctx_dir.join("state.json")).ok()?;
    serde_json::from_str(&s).ok()
}

fn write_state(ctx_dir: &Path, state: &ProjectContext) -> Result<()> {
    let s = serde_json::to_string_pretty(state).context("serialize state")?;
    std::fs::write(ctx_dir.join("state.json"), s)?;
    Ok(())
}

#[allow(dead_code)]
fn _touch(_: EnvironmentState) {}

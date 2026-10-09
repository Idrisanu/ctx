use anyhow::{Context, Result};
use ctx_core::{Checkpoint, Config, ProjectContext};
use std::fs;
use std::path::{Path, PathBuf};

/// Write fully to a temp file in the same directory, then rename.
/// A crash can leave the temp file behind but never a half-written target.
pub fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp-{}",
        path.extension().and_then(|e| e.to_str()).unwrap_or("tmp"),
        std::process::id()
    ));
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Held guard keeps an exclusive flock on `.ctx/.lock` until dropped.
/// Crash-safe: the OS releases the lock with the process.
pub struct CtxLock {
    _file: fs::File,
}

pub struct CtxDir {
    pub root: PathBuf,
}

impl CtxDir {
    pub fn at(root: impl AsRef<Path>) -> Self {
        CtxDir {
            root: root.as_ref().join(".ctx"),
        }
    }

    pub fn exists(&self) -> bool {
        self.root.is_dir()
    }

    pub fn discover(start: impl AsRef<Path>) -> Result<Option<CtxDir>> {
        let mut p = start.as_ref().canonicalize()?;
        loop {
            let candidate = p.join(".ctx");
            if candidate.is_dir() {
                return Ok(Some(CtxDir { root: candidate }));
            }
            match p.parent() {
                Some(parent) => p = parent.to_path_buf(),
                None => return Ok(None),
            }
        }
    }

    pub fn init(&self) -> Result<()> {
        if self.exists() {
            // idempotent: refresh scaffolding, don't fail
            for sub in [
                "context",
                "sessions",
                "snapshots",
                "decisions",
                "handoffs",
                "index",
            ] {
                fs::create_dir_all(self.root.join(sub))?;
            }
            return Ok(());
        }
        for sub in [
            "context",
            "sessions",
            "snapshots",
            "decisions",
            "handoffs",
            "index",
        ] {
            fs::create_dir_all(self.root.join(sub))?;
        }
        Ok(())
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("config.toml")
    }
    pub fn state_path(&self) -> PathBuf {
        self.root.join("state.json")
    }
    pub fn checkpoints_path(&self) -> PathBuf {
        self.root.join("index").join("checkpoints.json")
    }

    pub fn load_config(&self) -> Result<Config> {
        let s = fs::read_to_string(self.config_path()).context("read config.toml")?;
        Ok(toml::from_str(&s)?)
    }

    pub fn save_config(&self, cfg: &Config) -> Result<()> {
        atomic_write(&self.config_path(), toml::to_string_pretty(cfg)?.as_bytes())
    }

    pub fn load_state(&self) -> Result<ProjectContext> {
        let s = fs::read_to_string(self.state_path()).context("read state.json")?;
        let state: ProjectContext = serde_json::from_str(&s).context(
            "state.json is not valid ctx state (was it edited by hand? try `ctx init` in a fresh directory)",
        )?;
        if state.version > ctx_core::STATE_VERSION {
            anyhow::bail!(
                "state.json needs ctx v{}+ (this ctx writes v{}). Install a newer ctx, or back up and re-run `ctx init`.",
                state.version,
                ctx_core::STATE_VERSION
            );
        }
        Ok(state)
    }

    pub fn save_state(&self, state: &ProjectContext) -> Result<()> {
        let s = serde_json::to_string_pretty(state)?;
        atomic_write(&self.state_path(), s.as_bytes())
    }

    pub fn load_checkpoints(&self) -> Result<Vec<Checkpoint>> {
        let p = self.checkpoints_path();
        if !p.exists() {
            return Ok(vec![]);
        }
        let s = fs::read_to_string(p)?;
        Ok(serde_json::from_str(&s)?)
    }

    pub fn save_checkpoints(&self, cps: &[Checkpoint]) -> Result<()> {
        fs::create_dir_all(self.root.join("index"))?;
        atomic_write(
            &self.checkpoints_path(),
            serde_json::to_string_pretty(cps)?.as_bytes(),
        )
    }

    /// Exclusive inter-process lock for `.ctx/`. Hold the guard across a
    /// whole read-modify-write so concurrent ctx processes can't interleave.
    pub fn lock(&self) -> Result<CtxLock> {
        use fs2::FileExt;
        let path = self.root.join(".lock");
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .context("open .ctx lockfile")?;
        file.lock_exclusive().context("acquire .ctx lock")?;
        Ok(CtxLock { _file: file })
    }

    /// Load state, apply `f`, save atomically — all under the lock.
    /// Use for every state mutation so concurrent writers can't lose updates.
    pub fn update_state<F>(&self, f: F) -> Result<ProjectContext>
    where
        F: FnOnce(&mut ProjectContext) -> Result<()>,
    {
        let _guard = self.lock()?;
        let mut state = self.load_state()?;
        f(&mut state)?;
        self.save_state(&state)?;
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn init_and_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ctx-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let ctx = CtxDir::at(&dir);
        ctx.init().unwrap();
        let cfg = Config {
            project_name: "demo".into(),
            checkpoint_counter: 3,
            created_at: None,
            primary_agent: None,
        };
        ctx.save_config(&cfg).unwrap();
        let loaded = ctx.load_config().unwrap();
        assert_eq!(loaded.checkpoint_counter, 3);
        let _ = fs::remove_dir_all(&dir);
    }

    fn test_ctx(tag: &str) -> (CtxDir, PathBuf) {
        let dir = std::env::temp_dir().join(format!("ctx-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        (CtxDir::at(&dir), dir)
    }

    #[test]
    fn atomic_write_never_leaves_partial_target() {
        let (ctx, dir) = test_ctx("atomic");
        ctx.init().unwrap();
        let p = ctx.state_path();
        atomic_write(&p, b"{\"a\":1}").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"{\"a\":1}");
        // no temp droppings beside the target
        let leftovers: Vec<_> = fs::read_dir(ctx.root.clone())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .ok()
                    .map(|e| e.file_name().to_string_lossy().contains("tmp-"))
                    .unwrap_or(false)
            })
            .collect();
        assert!(leftovers.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lock_is_exclusive_across_handles() {
        use fs2::FileExt;
        let (ctx, dir) = test_ctx("lock");
        ctx.init().unwrap();
        let _guard = ctx.lock().unwrap();
        // second handle on the same lockfile must not acquire while held
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(ctx.root.join(".lock"))
            .unwrap();
        assert!(file.try_lock_exclusive().is_err());
        drop(_guard);
        assert!(file.try_lock_exclusive().is_ok());
        file.unlock().unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn damaged_state_fails_with_guidance() {
        let (ctx, dir) = test_ctx("damage");
        ctx.init().unwrap();
        fs::write(ctx.state_path(), "{not valid json").unwrap();
        let err = ctx.load_state().unwrap_err().to_string();
        assert!(err.contains("not valid ctx state"), "got: {}", err);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_state_applies_under_lock() {
        let (ctx, dir) = test_ctx("update");
        ctx.init().unwrap();
        let mut blank = ProjectContext::default();
        blank.project = "demo".into();
        ctx.save_state(&blank).unwrap();
        let back = ctx
            .update_state(|s| {
                s.objective = Some("ship it".into());
                Ok(())
            })
            .unwrap();
        assert_eq!(back.objective.as_deref(), Some("ship it"));
        assert_eq!(
            ctx.load_state().unwrap().objective.as_deref(),
            Some("ship it")
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

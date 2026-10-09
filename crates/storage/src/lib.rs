use anyhow::{Context, Result};
use ctx_core::{Checkpoint, Config, ProjectContext};
use std::fs;
use std::path::{Path, PathBuf};

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
        fs::write(self.config_path(), toml::to_string_pretty(cfg)?)?;
        Ok(())
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
        fs::write(self.state_path(), s)?;
        Ok(())
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
        fs::write(self.checkpoints_path(), serde_json::to_string_pretty(cps)?)?;
        Ok(())
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
}

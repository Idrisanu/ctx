use crate::SessionInfo;
use std::path::Path;

pub trait SessionReader: Send + Sync {
    fn name(&self) -> &'static str;
    /// Newest session for the given project directory, if any.
    fn latest(&self, project_dir: &Path) -> Option<SessionInfo>;
    /// Cheap freshness probe (unix seconds) for polling loops.
    /// Default falls back to a full parse; readers should override
    /// with an mtime-only check when possible.
    fn latest_mtime(&self, project_dir: &Path) -> Option<i64> {
        self.latest(project_dir).map(|s| s.updated_unix)
    }
}

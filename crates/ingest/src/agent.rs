use crate::SessionInfo;
use std::path::Path;

pub trait SessionReader: Send + Sync {
    fn name(&self) -> &'static str;
    /// Newest session for the given project directory, if any.
    fn latest(&self, project_dir: &Path) -> Option<SessionInfo>;
}

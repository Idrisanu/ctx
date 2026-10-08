use serde::{Deserialize, Serialize};

pub mod model {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct Decision {
        pub id: String,
        pub decision: String,
        pub reason: Option<String>,
        pub recorded_at: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct Task {
        pub title: String,
        pub status: String, // pending | in_progress | done
        pub notes: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct SessionMeta {
        pub agent: Option<String>,
        pub started_at: Option<String>,
        pub ended_at: Option<String>,
        pub task: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct EnvironmentState {
        pub runtimes: Vec<String>,
        pub package_managers: Vec<String>,
        pub docker: Option<bool>,
        pub env_vars_required: Vec<String>,
        pub env_vars_missing: Vec<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ProjectContext {
        #[serde(default = "default_version")]
        pub version: u32,
        pub project: String,
        pub objective: Option<String>,
        pub status: String,
        #[serde(default)]
        pub completed: Vec<String>,
        #[serde(default)]
        pub current_work: Vec<String>,
        #[serde(default)]
        pub decisions: Vec<Decision>,
        #[serde(default)]
        pub constraints: Vec<String>,
        #[serde(default)]
        pub files_changed: Vec<String>,
        #[serde(default)]
        pub errors: Vec<String>,
        pub next_action: Option<String>,
        #[serde(default)]
        pub tasks: Vec<Task>,
        #[serde(default)]
        pub sessions: Vec<SessionMeta>,
        #[serde(default)]
        pub environment: EnvironmentState,
        /// Compressed summary of the most recently ingested AI session.
        /// Set by `ctx ingest`; source tracked for honest provenance.
        #[serde(default)]
        pub ingested: Option<ctx_ingest_summary::Ingested>,
    }

    impl Default for ProjectContext {
        fn default() -> Self {
            Self {
                version: 1,
                project: String::new(),
                objective: None,
                status: String::new(),
                completed: vec![],
                current_work: vec![],
                decisions: vec![],
                constraints: vec![],
                files_changed: vec![],
                errors: vec![],
                next_action: None,
                tasks: vec![],
                sessions: vec![],
                environment: Default::default(),
                ingested: None,
            }
        }
    }

    fn default_version() -> u32 {
        1
    }

    /// Inline minimal copy to avoid core→ingest dependency cycle.
    pub mod ctx_ingest_summary {
        use serde::{Deserialize, Serialize};
        #[derive(Debug, Clone, Serialize, Deserialize, Default)]
        pub struct Ingested {
            pub agent: String,
            pub goal: Option<String>,
            pub last_user_message: Option<String>,
            pub last_assistant_excerpt: Option<String>,
            pub errors: Vec<String>,
            pub files_touched: Vec<String>,
            pub commands: Vec<String>,
            pub source: String,
        }
    }
    pub use ctx_ingest_summary::Ingested;

    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct Config {
        pub project_name: String,
        pub checkpoint_counter: u64,
        #[serde(default)]
        pub created_at: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Checkpoint {
        pub id: String,
        pub git_commit: Option<String>,
        pub branch: Option<String>,
        pub agent: Option<String>,
        pub task: Option<String>,
        pub files_changed: usize,
        #[serde(default)]
        pub files: Vec<String>,
        pub status: String,
        pub created_at: String,
        pub summary: Option<String>,
    }
}

pub use model::*;

pub fn next_checkpoint_id(counter: u64) -> String {
    format!("CTX-{:04}", counter)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoint_id_format() {
        assert_eq!(next_checkpoint_id(1), "CTX-0001");
        assert_eq!(next_checkpoint_id(1847), "CTX-1847");
    }
    #[test]
    fn state_roundtrip() {
        let mut s = ProjectContext::default();
        s.project = "demo".into();
        s.constraints.push("Use TypeScript".into());
        let j = serde_json::to_string(&s).unwrap();
        let back: ProjectContext = serde_json::from_str(&j).unwrap();
        assert_eq!(back.project, "demo");
        assert_eq!(back.constraints, vec!["Use TypeScript"]);
    }
}

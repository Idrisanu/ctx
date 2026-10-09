use serde::{Deserialize, Serialize};

pub mod model {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct Decision {
        pub id: String,
        pub decision: String,
        pub reason: Option<String>,
        pub recorded_at: Option<String>,
        /// Id of the decision that replaced this one (history kept, not deleted).
        #[serde(default)]
        pub superseded_by: Option<String>,
        /// Id of an active decision this one may contradict (human to resolve).
        #[serde(default)]
        pub conflicts_with: Option<String>,
    }

    impl Decision {
        pub fn active(&self) -> bool {
            self.superseded_by.is_none()
        }
    }

    const STOPWORDS: &[&str] = &[
        "the", "a", "an", "and", "or", "to", "for", "of", "in", "on", "with", "use", "using",
        "used", "all", "our", "we", "should", "will", "be", "is", "are", "it", "as", "by", "at",
        "from", "this", "that",
    ];

    fn content_tokens(s: &str) -> Vec<String> {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| t.len() >= 4 && !STOPWORDS.contains(t))
            .map(|t| t.to_string())
            .collect()
    }

    /// Heuristic: do two decision texts talk about the same topic while
    /// saying different things? Shared content tokens (len>=4, non-stopword)
    /// with non-identical texts. Cheap on purpose — false positives cost a
    /// glance, false negatives cost silent deformation.
    pub fn maybe_conflicts(a: &str, b: &str) -> bool {
        let ta = content_tokens(a);
        let tb = content_tokens(b);
        if ta.is_empty() || tb.is_empty() {
            return false;
        }
        if a.trim().eq_ignore_ascii_case(b.trim()) {
            return false;
        }
        ta.iter().any(|t| tb.contains(t))
    }

    /// Decisions still awaiting human review (flagged, not superseded).
    pub fn live_conflicts(state: &ProjectContext) -> Vec<&Decision> {
        state
            .decisions
            .iter()
            .filter(|d| d.active() && d.conflicts_with.is_some())
            .collect()
    }

    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct Task {
        pub title: String,
        pub status: String, // pending | in_progress | done
        pub notes: Option<String>,
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
        pub environment: EnvironmentState,
        /// Compressed summary of the most recently ingested AI session.
        /// Set by `ctx ingest`; source tracked for honest provenance.
        #[serde(default)]
        pub ingested: Option<ctx_ingest_summary::Ingested>,
        /// Commands quoted in the AI cooperative note. Filled by apply_note
        /// at display time; never persisted.
        #[serde(default, skip_serializing)]
        pub note_commands: Vec<String>,
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
                environment: Default::default(),
                ingested: None,
                note_commands: vec![],
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
            #[serde(default)]
            pub updated_unix: i64,
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

pub const STATE_VERSION: u32 = 1;

pub fn next_checkpoint_id(counter: u64) -> String {
    format!("CTX-{:04}", counter)
}

/// Allocate the next free checkpoint ID, skipping any that already exist
/// (e.g. after hand-edited config). Bumps the counter past collisions.
pub fn alloc_checkpoint_id(counter: &mut u64, existing: &[model::Checkpoint]) -> String {
    loop {
        *counter += 1;
        let id = next_checkpoint_id(*counter);
        if !existing.iter().any(|c| c.id == id) {
            return id;
        }
    }
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
    #[test]
    fn alloc_skips_collisions() {
        let existing = vec![
            Checkpoint {
                id: "CTX-0001".into(),
                git_commit: None,
                branch: None,
                agent: None,
                task: None,
                files_changed: 0,
                files: vec![],
                status: "auto".into(),
                created_at: "".into(),
                summary: None,
            },
            Checkpoint {
                id: "CTX-0002".into(),
                git_commit: None,
                branch: None,
                agent: None,
                task: None,
                files_changed: 0,
                files: vec![],
                status: "auto".into(),
                created_at: "".into(),
                summary: None,
            },
        ];
        let mut counter = 0;
        assert_eq!(alloc_checkpoint_id(&mut counter, &existing), "CTX-0003");
        assert_eq!(counter, 3);
    }
    #[test]
    fn conflict_heuristic() {
        // same topic, different choice -> flag
        assert!(maybe_conflicts(
            "Use PostgreSQL for the database",
            "Switch database to MongoDB"
        ));
        // unrelated topics -> quiet
        assert!(!maybe_conflicts("Use TypeScript", "Use PostgreSQL"));
        // identical -> not a conflict
        assert!(!maybe_conflicts("Use TypeScript", "use typescript"));
        // nothing substantive -> quiet
        assert!(!maybe_conflicts("Use it", "Do it"));
    }
}

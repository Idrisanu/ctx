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

    /// Conservative secret redaction for text about to be persisted into
    /// `.ctx/` (which is designed to be committed). Only high-confidence
    /// patterns are touched; anything ambiguous is left alone and the
    /// function documents that limit. Returns redacted text.
    ///
    /// Covered: OpenAI `sk-` keys, GitHub tokens, AWS access keys, Slack
    /// tokens, PEM private-key blocks, and `KEY=VALUE` assignments where
    /// the key name says secret/token/password and the value is ≥8 chars.
    /// NOT covered: bare hex strings, JWTs without a label, prose that
    /// merely mentions secrets. When in doubt, keep `.ctx/` private.
    pub fn redact_text(input: &str) -> String {
        let mut out = input.to_string();
        out = redact_prefixed(&out);
        out = redact_pem_blocks(&out);
        out = redact_assignments(&out);
        out
    }

    fn redact_prefixed(s: &str) -> String {
        // (prefix matcher, label)
        const PREFIXES: &[(&str, &str)] = &[
            ("sk-", "OPENAI_KEY"),
            ("sk-proj-", "OPENAI_KEY"),
            ("ghp_", "GITHUB_TOKEN"),
            ("gho_", "GITHUB_TOKEN"),
            ("ghu_", "GITHUB_TOKEN"),
            ("ghs_", "GITHUB_TOKEN"),
            ("ghr_", "GITHUB_TOKEN"),
            ("github_pat_", "GITHUB_TOKEN"),
            ("AKIA", "AWS_KEY"),
            ("xoxb-", "SLACK_TOKEN"),
            ("xoxp-", "SLACK_TOKEN"),
            ("xoxa-", "SLACK_TOKEN"),
            ("xoxr-", "SLACK_TOKEN"),
            ("xoxs-", "SLACK_TOKEN"),
        ];
        let mut result = String::with_capacity(s.len());
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let mut matched: Option<(&str, usize)> = None;
            for (prefix, label) in PREFIXES {
                if s[i..].starts_with(prefix) {
                    // consume the prefix plus trailing token chars
                    let mut j = i + prefix.len();
                    while j < bytes.len()
                        && (bytes[j].is_ascii_alphanumeric()
                            || bytes[j] == b'-'
                            || bytes[j] == b'_')
                    {
                        j += 1;
                    }
                    // require a minimum secret length past the prefix
                    if j - (i + prefix.len()) >= 8 {
                        matched = Some((label, j));
                        break;
                    }
                }
            }
            if let Some((label, end)) = matched {
                result.push_str("[REDACTED:");
                result.push_str(label);
                result.push(']');
                i = end;
            } else {
                // advance one char (byte-safe: ASCII fast path, else whole char)
                let c = s[i..].chars().next().unwrap();
                result.push(c);
                i += c.len_utf8();
            }
        }
        result
    }

    fn redact_pem_blocks(s: &str) -> String {
        let mut out = String::new();
        let mut rest = s;
        while let Some(start) = rest.find("-----BEGIN ") {
            if let Some(end) = rest[start..].find("-----END ") {
                // find end of the END line
                let tail = &rest[start..];
                if let Some(nl) = tail[end..].find('\n') {
                    let kind = tail["-----BEGIN ".len()..]
                        .split("-----")
                        .next()
                        .unwrap_or("KEY");
                    out.push_str(&rest[..start]);
                    out.push_str("[REDACTED:");
                    out.push_str(kind.trim());
                    out.push(']');
                    rest = &tail[end + nl + 1..];
                    continue;
                }
            }
            break;
        }
        out.push_str(rest);
        out
    }

    fn is_secret_name(word: &str) -> bool {
        let w = word.to_lowercase();
        [
            "secret", "token", "password", "passwd", "pwd", "api_key", "apikey",
        ]
        .iter()
        .any(|k| w.contains(k))
    }

    fn redact_assignments(s: &str) -> String {
        let mut lines: Vec<String> = Vec::new();
        for line in s.split_inclusive('\n') {
            let has_nl = line.ends_with('\n');
            let body = line.trim_end_matches('\n');
            // look for NAME=VALUE or NAME: VALUE with a secret-ish NAME
            let mut replaced = body.to_string();
            for sep in ['=', ':'] {
                if let Some(pos) = body.find(sep) {
                    let (name, mut value) = body.split_at(pos);
                    value = &value[1..];
                    let name_word = name
                        .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
                        .next()
                        .unwrap_or("");
                    let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
                    if !name_word.is_empty()
                        && is_secret_name(name_word)
                        && value.len() >= 8
                        && !value.starts_with("[REDACTED")
                    {
                        replaced = format!(
                            "{}{}[REDACTED]",
                            &body[..pos + 1],
                            if body.as_bytes().get(pos + 1) == Some(&b' ') {
                                " "
                            } else {
                                ""
                            }
                        );
                        break;
                    }
                }
            }
            if has_nl {
                replaced.push('\n');
            }
            lines.push(replaced);
        }
        lines.concat()
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
            #[serde(default)]
            pub progress: Option<String>,
            #[serde(default)]
            pub tokens_total: Option<u64>,
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
        /// Agent declared at init (`ctx init --agent X`). Bare `ctx resume`
        /// defaults to this flavor instead of generic.
        #[serde(default)]
        pub primary_agent: Option<String>,
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
    #[test]
    fn redact_known_prefixes() {
        assert_eq!(
            redact_text("key sk-abc1234567890xyz done"),
            "key [REDACTED:OPENAI_KEY] done"
        );
        assert_eq!(
            redact_text("token ghp_abcdefghij1234567890"),
            "token [REDACTED:GITHUB_TOKEN]"
        );
        assert_eq!(
            redact_text("id AKIAIOSFODNN7EXAMPLE"),
            "id [REDACTED:AWS_KEY]"
        );
        // too short after prefix -> left alone
        assert_eq!(redact_text("see sk-abc end"), "see sk-abc end");
    }
    #[test]
    fn redact_assignments_and_pem() {
        assert_eq!(
            redact_text("OPENAI_API_KEY=sk-fake-SECRET-abc123"),
            "OPENAI_API_KEY=[REDACTED:OPENAI_KEY]"
        );
        assert_eq!(
            redact_text("password: hunter2-hunter2"),
            "password: [REDACTED]"
        );
        // normal prose untouched
        assert_eq!(
            redact_text("token limit hit mid-milestone"),
            "token limit hit mid-milestone"
        );
        assert_eq!(
            redact_text("Next action: fix the expiry check"),
            "Next action: fix the expiry check"
        );
        let pem = "key:\n-----BEGIN RSA PRIVATE KEY-----\nMIIBOgIBAA\n-----END RSA PRIVATE KEY-----\ndone";
        let out = redact_text(pem);
        assert!(out.contains("[REDACTED:RSA PRIVATE KEY]"));
        assert!(!out.contains("MIIBOgIBAA"));
        assert!(out.contains("done"));
    }
}

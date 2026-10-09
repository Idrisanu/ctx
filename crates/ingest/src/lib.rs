use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub mod agent;
pub mod claude;
pub mod gemini;
pub mod opencode;

pub use agent::SessionReader;

/// Everything we need to know about one AI coding session,
/// normalized across agents.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionInfo {
    pub agent: String,
    pub session_id: String,
    pub project_dir: PathBuf,
    pub updated_unix: i64,
    pub first_user_message: Option<String>,
    pub last_user_message: Option<String>,
    pub last_assistant_excerpt: Option<String>,
    /// Latest progress summary, when the agent reports one
    /// (e.g. Gemini update_topic). None when absent.
    #[serde(default)]
    pub progress: Option<String>,
    /// Summed message token totals, when the store reports them.
    #[serde(default)]
    pub tokens_total: Option<u64>,
    pub errors: Vec<String>,
    pub files_touched: Vec<String>,
    pub commands: Vec<String>,
}

/// Compressed, honest summary safe to persist in .ctx (no raw transcript).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompressedSession {
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
}

/// All readers known to this build.
pub fn registry() -> Vec<Box<dyn SessionReader>> {
    vec![
        Box::new(claude::ClaudeCodeReader),
        Box::new(opencode::OpenCodeReader),
        Box::new(gemini::GeminiReader),
    ]
}

/// Find the newest session across all readers for `project_dir`.
pub fn latest_for(project_dir: &Path) -> Option<SessionInfo> {
    registry()
        .iter()
        .filter_map(|r| r.latest(project_dir))
        .max_by_key(|s| s.updated_unix)
}

/// Reader status for `ctx agents`.
pub fn detected(project_dir: &Path) -> Vec<(String, bool)> {
    registry()
        .iter()
        .map(|r| (r.name().to_string(), r.latest(project_dir).is_some()))
        .collect()
}

/// Compress a session into a short, honest summary block.
pub fn compress(s: &SessionInfo) -> CompressedSession {
    CompressedSession {
        agent: s.agent.clone(),
        goal: s.first_user_message.clone(),
        last_user_message: s.last_user_message.clone(),
        last_assistant_excerpt: s.last_assistant_excerpt.clone(),
        progress: s.progress.clone(),
        tokens_total: s.tokens_total,
        errors: s.errors.iter().take(5).cloned().collect(),
        files_touched: s.files_touched.iter().take(20).cloned().collect(),
        commands: s.commands.iter().take(10).cloned().collect(),
        source: format!("{} session {}", s.agent, s.session_id),
    }
}

pub fn excerpt(s: &str, max: usize) -> String {
    let t: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        format!("{}…", t)
    } else {
        t
    }
}

/// Best-effort text pull from Claude-style `content` (string or blocks).
pub fn content_text(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

pub fn _read_jsonl<F: FnMut(&serde_json::Value)>(path: &Path, mut f: F) -> Result<()> {
    let s = std::fs::read_to_string(path)?;
    for line in s.lines() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            f(&v);
        }
    }
    Ok(())
}

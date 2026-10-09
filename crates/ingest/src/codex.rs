use crate::{excerpt, SessionInfo, SessionReader};
use std::path::{Path, PathBuf};

pub struct CodexReader;

fn db_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(home.join(".codex").join("state_5.sqlite"))
}

impl SessionReader for CodexReader {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn latest(&self, project_dir: &Path) -> Option<SessionInfo> {
        let path = db_path()?;
        if !path.exists() {
            return None;
        }
        self.latest_from_db(&path, project_dir)
    }

    fn latest_mtime(&self, project_dir: &Path) -> Option<i64> {
        let path = db_path()?;
        if !path.exists() {
            return None;
        }
        self.mtime_from_db(&path, project_dir)
    }
}

fn canon_dir(project_dir: &Path) -> String {
    project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf())
        .to_string_lossy()
        .to_string()
}

/// Split out so tests can point at a fixture database.
impl CodexReader {
    pub fn mtime_from_db(&self, db: &Path, project_dir: &Path) -> Option<i64> {
        let conn = rusqlite::Connection::open(db).ok()?;
        let dir = canon_dir(project_dir);
        conn.query_row(
            "SELECT max(updated_at) FROM threads WHERE cwd = ?1",
            [&dir],
            |r| r.get::<_, Option<i64>>(0),
        )
        .ok()?
    }

    pub fn latest_from_db(&self, db: &Path, project_dir: &Path) -> Option<SessionInfo> {
        let conn = rusqlite::Connection::open(db).ok()?;
        let dir = canon_dir(project_dir);
        let (thread_id, rollout, updated): (String, String, Option<i64>) = conn
            .query_row(
                "SELECT id, rollout_path, updated_at FROM threads WHERE cwd = ?1 ORDER BY updated_at DESC LIMIT 1",
                [&dir],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok()?;
        let _ = thread_id;
        parse(Path::new(&rollout), project_dir, updated.unwrap_or(0))
    }
}

fn message_text(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| {
                b.get("text").and_then(|t| t.as_str()).filter(|_| {
                    matches!(
                        b.get("type").and_then(|t| t.as_str()),
                        Some("input_text") | Some("output_text")
                    )
                })
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Skip injected instruction dumps — they drown the real user prompt.
fn is_system_dump(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("<session_context>")
        || t.starts_with("# AGENTS.md instructions")
        || t.starts_with("<INSTRUCTIONS>")
        || t.starts_with("<skills_instructions>")
}

/// Pull cmd:"..." out of an exec_tool JS wrapper, honouring backslash escapes.
fn extract_cmd(input: &str) -> Option<String> {
    let start = input.find("cmd:\"")? + 5;
    let mut out = String::new();
    let mut esc = false;
    for c in input[start..].chars() {
        if esc {
            out.push(c);
            esc = false;
        } else if c == '\\' {
            esc = true;
        } else if c == '"' {
            break;
        } else {
            out.push(c);
        }
    }
    if out.trim().is_empty() {
        None
    } else {
        Some(out)
    }
}

fn parse(path: &Path, project_dir: &Path, updated: i64) -> Option<SessionInfo> {
    let mut first_user: Option<String> = None;
    let mut last_user: Option<String> = None;
    let mut last_assistant: Option<String> = None;
    let mut errors: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    let mut commands: Vec<String> = Vec::new();
    let mut tokens: u64 = 0;

    crate::_read_jsonl(path, |v| {
        let kind = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match kind {
            "response_item" => {
                let p = v.get("payload").cloned().unwrap_or(serde_json::Value::Null);
                match p.get("type").and_then(|t| t.as_str()) {
                    Some("message") => {
                        let text =
                            message_text(p.get("content").unwrap_or(&serde_json::Value::Null));
                        if text.is_empty() || is_system_dump(&text) {
                            return;
                        }
                        match p.get("role").and_then(|r| r.as_str()) {
                            Some("user") => {
                                if first_user.is_none() {
                                    first_user = Some(excerpt(&text, 300));
                                }
                                last_user = Some(excerpt(&text, 300));
                            }
                            Some("assistant") => {
                                last_assistant = Some(excerpt(&text, 400));
                            }
                            _ => {}
                        }
                    }
                    Some("custom_tool_call") => {
                        let name = p.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        if name == "exec" {
                            if let Some(input) = p.get("input").and_then(|i| i.as_str()) {
                                if let Some(cmd) = extract_cmd(input) {
                                    for f in paths_in(&cmd, project_dir) {
                                        if !files.contains(&f) {
                                            files.push(f);
                                        }
                                    }
                                    commands.push(excerpt(&cmd, 120));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            "token_usage_record" => {
                if let Some(t) = p_total(v) {
                    tokens = tokens.saturating_add(t);
                }
            }
            "event_msg" => {
                let ptype = v
                    .get("payload")
                    .and_then(|p| p.get("type"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                if ptype.contains("error") || ptype.contains("failed") {
                    errors.push(excerpt(&serde_json::to_string(&v).unwrap_or_default(), 200));
                }
            }
            _ => {}
        }
    })
    .ok();

    Some(SessionInfo {
        agent: "codex".into(),
        session_id: path
            .file_stem()
            .map(|s| s.to_string_lossy().into())
            .unwrap_or_default(),
        project_dir: project_dir.to_path_buf(),
        updated_unix: updated,
        first_user_message: first_user,
        last_user_message: last_user,
        last_assistant_excerpt: last_assistant,
        progress: None,
        tokens_total: if tokens > 0 { Some(tokens) } else { None },
        errors,
        files_touched: files,
        commands,
    })
}

fn p_total(v: &serde_json::Value) -> Option<u64> {
    v.get("payload")?
        .get("usage")?
        .get("total_tokens")?
        .as_u64()
}

fn paths_in(cmd: &str, project_dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for tok in cmd.split(|c: char| c.is_whitespace() || "`'\"".contains(c)) {
        let t = tok.trim_end_matches(&[',', ';', ')', ']', '.'][..]);
        // Absolute path: keep only when under the project (relativized).
        // Relative path with a slash: exec wrappers run with the project
        // as workdir, so resolve it there. Bare words (npm, git) ignored.
        let rel = if t.starts_with('/') {
            let after_slash = t.rsplit('/').next().unwrap_or("");
            if !after_slash.contains('.') {
                continue;
            }
            match Path::new(t).strip_prefix(project_dir) {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(_) => continue,
            }
        } else if t.contains('/') {
            let after_slash = t.rsplit('/').next().unwrap_or("");
            if !after_slash.contains('.') {
                continue;
            }
            t.trim_start_matches("./").to_string()
        } else {
            continue;
        };
        if !rel.is_empty() && !out.contains(&rel) {
            out.push(rel);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_db(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
        let proj = dir.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let rollout = dir.join("rollout-test.jsonl");
        let edit_path = format!("{}/src/index.ts", proj.to_string_lossy());
        let lines = vec![
            r#"{"timestamp":"2026-10-08T19:13:57Z","ordinal":0,"type":"session_meta","payload":{}}"#.to_string(),
            r#"{"timestamp":"2026-10-08T19:14:00Z","ordinal":1,"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"divide into 3 milestones, work on milestone 1"}]}}"#.to_string(),
            format!(
                r#"{{"timestamp":"2026-10-08T19:14:02Z","ordinal":2,"type":"response_item","payload":{{"type":"custom_tool_call","name":"exec","status":"completed","input":"await tools.exec_command({{cmd:\"npm run build && echo {0}\",\"workdir\":\"/proj\"}})"}}}}"#,
                edit_path
            ),
            r#"{"timestamp":"2026-10-08T19:14:05Z","ordinal":3,"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"milestone 1 scaffolded"}]}}"#.to_string(),
            r#"{"timestamp":"2026-10-08T19:15:00Z","ordinal":4,"type":"token_usage_record","payload":{"usage":{"total_tokens":1234}}}"#.to_string(),
        ];
        std::fs::write(&rollout, lines.join("\n")).unwrap();
        let db = dir.join("state.db");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT NOT NULL, cwd TEXT NOT NULL, updated_at INTEGER, tokens_used INTEGER);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO threads (id, rollout_path, cwd, updated_at, tokens_used) VALUES ('t1', ?1, ?2, 1791487000, 1234)",
            rusqlite::params![rollout.to_string_lossy().to_string(), proj.to_string_lossy().to_string()],
        )
        .unwrap();
        (db, proj)
    }

    #[test]
    fn parses_codex_rollout() {
        let dir = std::env::temp_dir().join(format!("ctx-codex-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (db, proj) = fixture_db(&dir);

        let r = CodexReader;
        assert_eq!(r.mtime_from_db(&db, &proj), Some(1791487000));
        let s = r.latest_from_db(&db, &proj).unwrap();
        assert_eq!(s.agent, "codex");
        assert!(s
            .first_user_message
            .as_deref()
            .unwrap()
            .contains("milestone 1"));
        assert!(s.commands.iter().any(|c| c.contains("npm run build")));
        assert!(s.files_touched.iter().any(|f| f == "src/index.ts"));
        // backtick-wrapped mentions resolve cleanly
        assert!(
            paths_in("cat `.ctx/handoffs/current.md`.", &proj) == vec![".ctx/handoffs/current.md"]
        );
        assert_eq!(
            s.last_assistant_excerpt.as_deref(),
            Some("milestone 1 scaffolded")
        );
        assert_eq!(s.tokens_total, Some(1234));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

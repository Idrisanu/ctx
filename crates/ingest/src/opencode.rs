use crate::{excerpt, SessionInfo, SessionReader};
use std::path::{Path, PathBuf};

pub struct OpenCodeReader;

fn db_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(home.join(".local/share/opencode/opencode.db"))
}

impl SessionReader for OpenCodeReader {
    fn name(&self) -> &'static str {
        "opencode"
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

/// Split out so tests can point at a fixture database.
impl OpenCodeReader {
    pub fn mtime_from_db(&self, db: &Path, project_dir: &Path) -> Option<i64> {
        let conn = rusqlite::Connection::open(db).ok()?;
        let dir = project_dir
            .canonicalize()
            .unwrap_or_else(|_| project_dir.to_path_buf())
            .to_string_lossy()
            .to_string();
        let ms: Option<i64> = conn
            .query_row(
                "SELECT max(time_updated) FROM session_v2 WHERE directory = ?1",
                [&dir],
                |r| r.get(0),
            )
            .ok()?;
        ms.map(|m| m / 1000)
    }

    pub fn latest_from_db(&self, db: &Path, project_dir: &Path) -> Option<SessionInfo> {
        let conn = rusqlite::Connection::open(db).ok()?;
        let dir = project_dir
            .canonicalize()
            .unwrap_or_else(|_| project_dir.to_path_buf())
            .to_string_lossy()
            .to_string();

        let mut stmt = conn
            .prepare("SELECT id, directory, time_updated FROM session_v2 WHERE directory = ?1 ORDER BY time_updated DESC LIMIT 1")
            .ok()?;
        let mut rows = stmt.query([&dir]).ok()?;
        let (session_id, updated_ms): (String, Option<i64>) = {
            let row = rows.next().ok()??;
            (row.get(0).ok()?, row.get(2).ok())
        };

        let mut first_user: Option<String> = None;
        let mut last_user: Option<String> = None;
        let mut last_assistant: Option<String> = None;
        let mut errors: Vec<String> = Vec::new();
        let mut files: Vec<String> = Vec::new();
        let mut commands: Vec<String> = Vec::new();

        let mut mstmt = conn
            .prepare(
                "SELECT type, data FROM session_message WHERE session_id = ?1 ORDER BY seq ASC",
            )
            .ok()?;
        let rows = mstmt
            .query_map([&session_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .ok()?;
        for (t, data) in rows.flatten() {
            let v: serde_json::Value = match serde_json::from_str(&data) {
                Ok(v) => v,
                Err(_) => continue,
            };
            match t.as_str() {
                "user" => {
                    let text = extract_text(&v);
                    if !text.is_empty() {
                        if first_user.is_none() {
                            first_user = Some(excerpt(&text, 300));
                        }
                        last_user = Some(excerpt(&text, 300));
                    }
                }
                "assistant" => {
                    let text = extract_text(&v);
                    if !text.is_empty() {
                        last_assistant = Some(excerpt(&text, 400));
                    }
                    if let Some(e) = v
                        .get("error")
                        .and_then(|e| e.get("type"))
                        .and_then(|t| t.as_str())
                    {
                        errors.push(e.to_string());
                    }
                    parse_tool_blocks(&v, project_dir, &mut commands, &mut files, &mut errors);
                }
                _ => {}
            }
        }

        Some(SessionInfo {
            agent: "opencode".into(),
            session_id,
            project_dir: project_dir.to_path_buf(),
            updated_unix: updated_ms.unwrap_or(0) / 1000,
            first_user_message: first_user,
            last_user_message: last_user,
            last_assistant_excerpt: last_assistant,
            errors,
            files_touched: dedup(files),
            commands,
        })
    }
}

/// Extract shell commands, edited files, and failed exits from
/// OpenCode assistant `tool` content blocks.
fn parse_tool_blocks(
    v: &serde_json::Value,
    project_dir: &Path,
    commands: &mut Vec<String>,
    files: &mut Vec<String>,
    errors: &mut Vec<String>,
) {
    let Some(blocks) = v.get("content").and_then(|c| c.as_array()) else {
        return;
    };
    for b in blocks {
        if b.get("type").and_then(|t| t.as_str()) != Some("tool") {
            continue;
        }
        let name = b.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let state = b.get("state").cloned().unwrap_or(serde_json::Value::Null);
        let input = state
            .get("input")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        match name {
            "shell" | "bash" => {
                if let Some(cmd) = input.get("command").and_then(|c| c.as_str()) {
                    if !cmd.trim().is_empty() {
                        commands.push(excerpt(cmd, 120));
                    }
                    let exit = state
                        .get("metadata")
                        .and_then(|m| m.get("exit"))
                        .and_then(|e| e.as_i64());
                    if exit.map(|e| e != 0).unwrap_or(false) {
                        errors.push(format!(
                            "command exited {}: {}",
                            exit.unwrap(),
                            excerpt(cmd, 100)
                        ));
                    }
                }
            }
            "edit" | "write" | "read" => {
                let f = ["filePath", "file", "path"]
                    .iter()
                    .filter_map(|k| input.get(k).and_then(|f| f.as_str()))
                    .next();
                if let Some(f) = f {
                    // store relative to the project when possible (portability)
                    let rel = Path::new(f)
                        .strip_prefix(project_dir)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| f.to_string());
                    if !files.contains(&rel) {
                        files.push(rel);
                    }
                }
            }
            _ => {}
        }
    }
}

fn dedup(v: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for s in v {
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

fn extract_text(v: &serde_json::Value) -> String {
    if let Some(t) = v.get("text").and_then(|t| t.as_str()) {
        return t.to_string();
    }
    if let Some(c) = v.get("content") {
        match c {
            serde_json::Value::String(s) => return s.clone(),
            serde_json::Value::Array(blocks) => {
                return blocks
                    .iter()
                    .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");
            }
            _ => {}
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_db(dir: &std::path::Path) -> std::path::PathBuf {
        let db = dir.join("opencode.db");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE session_v2 (id TEXT PRIMARY KEY, directory TEXT NOT NULL, time_updated INTEGER);
             CREATE TABLE session_message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, type TEXT NOT NULL, seq INTEGER NOT NULL, data TEXT NOT NULL);",
        )
        .unwrap();
        let proj = dir.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        conn.execute(
            "INSERT INTO session_v2 (id, directory, time_updated) VALUES ('ses1', ?1, 1791500000000)",
            [proj.to_string_lossy().to_string()],
        )
        .unwrap();
        let user = r#"{"time":{"created":1},"text":"build the api"}"#;
        let tool_ok = r#"{"time":{"created":2},"content":[{"type":"tool","name":"shell","state":{"status":"completed","input":{"command":"cargo test"},"metadata":{"exit":0}}}]}"#;
        let tool_fail = r#"{"time":{"created":3},"content":[{"type":"tool","name":"shell","state":{"status":"completed","input":{"command":"npm run build"},"metadata":{"exit":1}}}]}"#;
        let edit_path = format!("{}/src/main.rs", proj.to_string_lossy());
        let edit = format!(
            "{{\"time\":{{\"created\":4}},\"content\":[{{\"type\":\"tool\",\"name\":\"edit\",\"state\":{{\"status\":\"completed\",\"input\":{{\"path\":\"{}\"}}}}}}]}}",
            edit_path
        );
        for (i, (t, d)) in [
            ("user", user),
            ("assistant", tool_ok),
            ("assistant", tool_fail),
            ("assistant", &edit),
        ]
        .iter()
        .enumerate()
        {
            conn.execute(
                "INSERT INTO session_message (id, session_id, type, seq, data) VALUES (?1, 'ses1', ?2, ?3, ?4)",
                rusqlite::params![format!("m{}", i), t, i as i64, d],
            )
            .unwrap();
        }
        proj
    }

    #[test]
    fn parses_commands_files_and_failed_exits() {
        let dir = std::env::temp_dir().join(format!("ctx-opencode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let proj = fixture_db(&dir);

        let r = OpenCodeReader;
        assert_eq!(
            r.mtime_from_db(&dir.join("opencode.db"), &proj),
            Some(1791500000)
        );
        let s = r.latest_from_db(&dir.join("opencode.db"), &proj).unwrap();
        assert_eq!(s.first_user_message.as_deref(), Some("build the api"));
        assert!(s.commands.iter().any(|c| c.contains("cargo test")));
        assert!(s.commands.iter().any(|c| c.contains("npm run build")));
        assert!(s.errors.iter().any(|e| e.contains("exited 1")));
        assert!(s.files_touched.iter().any(|f| f == "src/main.rs"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

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
        let conn = rusqlite::Connection::open(&path).ok()?;
        let dir = project_dir
            .canonicalize()
            .ok()?
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
            files_touched: vec![],
            commands: vec![],
        })
    }
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

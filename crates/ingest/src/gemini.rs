use crate::{content_text, excerpt, SessionInfo, SessionReader};
use std::path::{Path, PathBuf};

pub struct GeminiReader;

fn tmp_root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".gemini").join("tmp"))
}

/// Every stored project: (slug dir, canonical project dir from .project_root).
fn stored_projects(root: &Path) -> Vec<(PathBuf, PathBuf)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let slug = entry.path();
        if !slug.is_dir() {
            continue;
        }
        let marker = slug.join(".project_root");
        let Ok(content) = std::fs::read_to_string(&marker) else {
            continue;
        };
        let raw = content.trim();
        if raw.is_empty() {
            continue;
        }
        let canon = Path::new(raw)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(raw));
        out.push((slug, canon));
    }
    out
}

fn newest_chat(slug: &Path) -> Option<(PathBuf, i64)> {
    let chats = slug.join("chats");
    let mut best: Option<(PathBuf, i64)> = None;
    for entry in std::fs::read_dir(&chats).ok()? {
        let p = entry.ok()?.path();
        let is_chat = p
            .extension()
            .map(|e| e == "jsonl" || e == "json")
            .unwrap_or(false);
        if !is_chat {
            continue;
        }
        let m = std::fs::metadata(&p)
            .ok()?
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs() as i64;
        if best.as_ref().map(|b| m > b.1).unwrap_or(true) {
            best = Some((p, m));
        }
    }
    best
}

impl SessionReader for GeminiReader {
    fn name(&self) -> &'static str {
        "gemini"
    }

    fn latest(&self, project_dir: &Path) -> Option<SessionInfo> {
        let root = tmp_root()?;
        self.latest_in(&root, project_dir)
    }

    fn latest_mtime(&self, project_dir: &Path) -> Option<i64> {
        let root = tmp_root()?;
        self.mtime_in(&root, project_dir)
    }
}

impl GeminiReader {
    /// Split out so tests can point at a fixture tmp dir.
    pub fn latest_in(&self, root: &Path, project_dir: &Path) -> Option<SessionInfo> {
        let canon = project_dir
            .canonicalize()
            .unwrap_or_else(|_| project_dir.to_path_buf());
        let (slug, _) = stored_projects(root)
            .into_iter()
            .find(|(_, dir)| dir == &canon)?;
        let (path, updated) = newest_chat(&slug)?;
        parse(&path, project_dir, updated)
    }

    pub fn mtime_in(&self, root: &Path, project_dir: &Path) -> Option<i64> {
        let canon = project_dir
            .canonicalize()
            .unwrap_or_else(|_| project_dir.to_path_buf());
        let (slug, _) = stored_projects(root)
            .into_iter()
            .find(|(_, dir)| dir == &canon)?;
        newest_chat(&slug).map(|(_, m)| m)
    }
}

/// Flatten a chat file into message records (top-level + $set batches).
fn records(path: &Path) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let Ok(content) = std::fs::read_to_string(path) else {
        return out;
    };
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(msgs) = v
            .get("$set")
            .and_then(|s| s.get("messages"))
            .and_then(|m| m.as_array())
        {
            out.extend(msgs.iter().cloned());
        } else if v.get("type").and_then(|t| t.as_str()).is_some() {
            out.push(v);
        }
    }
    out
}

fn message_text(m: &serde_json::Value) -> String {
    match m.get("content") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => content_text(m.get("content").unwrap_or(&serde_json::Value::Null)),
    }
}

fn parse(path: &Path, project_dir: &Path, updated: i64) -> Option<SessionInfo> {
    let mut first_user: Option<String> = None;
    let mut last_user: Option<String> = None;
    let mut last_assistant: Option<String> = None;
    let mut progress: Option<String> = None;
    let mut errors: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    let mut commands: Vec<String> = Vec::new();
    let mut tokens: u64 = 0;

    for m in records(path) {
        let kind = m.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match kind {
            "user" => {
                let text = message_text(&m);
                if text.trim_start().starts_with("<session_context>") {
                    continue;
                }
                if !text.is_empty() {
                    if first_user.is_none() {
                        first_user = Some(excerpt(&text, 300));
                    }
                    last_user = Some(excerpt(&text, 300));
                }
            }
            "gemini" => {
                let text = message_text(&m);
                if !text.is_empty() {
                    last_assistant = Some(excerpt(&text, 400));
                }
                if let Some(t) = m
                    .get("tokens")
                    .and_then(|t| t.get("total"))
                    .and_then(|t| t.as_u64())
                {
                    tokens = tokens.saturating_add(t);
                }
                for tc in m
                    .get("toolCalls")
                    .and_then(|t| t.as_array())
                    .map(|a| a.as_slice())
                    .unwrap_or(&[])
                {
                    let name = tc.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let args = tc.get("args").cloned().unwrap_or(serde_json::Value::Null);
                    match name {
                        "run_shell_command" => {
                            if let Some(cmd) = args.get("command").and_then(|c| c.as_str()) {
                                if !cmd.trim().is_empty() {
                                    commands.push(excerpt(cmd, 120));
                                }
                            }
                        }
                        "write_file" | "replace" | "read_file" => {
                            let f = ["file_path", "path", "file"]
                                .iter()
                                .filter_map(|k| args.get(k).and_then(|f| f.as_str()))
                                .next();
                            if let Some(f) = f {
                                let rel = Path::new(f)
                                    .strip_prefix(project_dir)
                                    .map(|p| p.to_string_lossy().to_string())
                                    .unwrap_or_else(|_| f.to_string());
                                if !files.contains(&rel) {
                                    files.push(rel);
                                }
                            }
                        }
                        "update_topic" => {
                            if let Some(s) = args.get("summary").and_then(|s| s.as_str()) {
                                if !s.trim().is_empty() {
                                    progress = Some(excerpt(s, 400));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            "error" => {
                let text = message_text(&m);
                if !text.is_empty() {
                    errors.push(excerpt(&text, 200));
                }
            }
            _ => {}
        }
    }

    Some(SessionInfo {
        agent: "gemini".into(),
        session_id: path
            .file_stem()
            .map(|s| s.to_string_lossy().into())
            .unwrap_or_default(),
        project_dir: project_dir.to_path_buf(),
        updated_unix: updated,
        first_user_message: first_user,
        last_user_message: last_user,
        last_assistant_excerpt: last_assistant,
        progress,
        tokens_total: if tokens > 0 { Some(tokens) } else { None },
        errors,
        files_touched: files,
        commands,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_tmp(dir: &std::path::Path) -> std::path::PathBuf {
        let proj = dir.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let slug = dir.join("ctx-demo2");
        std::fs::create_dir_all(slug.join("chats")).unwrap();
        std::fs::write(
            slug.join(".project_root"),
            proj.to_string_lossy().to_string(),
        )
        .unwrap();
        let chat = slug.join("chats").join("session-test.jsonl");
        let edit_path = format!("{}/src/app/page.tsx", proj.to_string_lossy());
        let lines = vec![
            r#"{"sessionId":"s1","kind":"main"}"#.to_string(),
            r#"{"id":"m1","timestamp":"2026-10-09T09:46:44Z","type":"user","content":[{"text":"build me a landing page"}]}"#.to_string(),
            format!(
                r#"{{"id":"m2","timestamp":"2026-10-09T09:50:14Z","type":"gemini","content":"","tokens":{{"total":17690}},"toolCalls":[{{"name":"run_shell_command","args":{{"command":"npx create-next-app . --yes"}}}},{{"name":"write_file","args":{{"file_path":"{}"}}}}]}}"#,
                edit_path
            ),
            r#"{"id":"m3","timestamp":"2026-10-09T09:54:24Z","type":"error","content":"[API Error: You have exhausted your daily quota on this model.]"}"#.to_string(),
        ];
        std::fs::write(&chat, lines.join("\n")).unwrap();
        proj
    }

    #[test]
    fn parses_gemini_session() {
        let dir = std::env::temp_dir().join(format!("ctx-gemini-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let proj = fixture_tmp(&dir);

        let r = GeminiReader;
        assert!(r.mtime_in(&dir, &proj).is_some());
        let s = r.latest_in(&dir, &proj).unwrap();
        assert_eq!(s.agent, "gemini");
        assert_eq!(
            s.first_user_message.as_deref(),
            Some("build me a landing page")
        );
        assert!(s.commands.iter().any(|c| c.contains("create-next-app")));
        assert!(s.files_touched.iter().any(|f| f == "src/app/page.tsx"));
        assert!(s
            .errors
            .iter()
            .any(|e| e.contains("exhausted your daily quota")));
        assert_eq!(s.tokens_total, Some(17690));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

use crate::{content_text, excerpt, SessionInfo, SessionReader};
use std::path::{Path, PathBuf};

pub struct ClaudeCodeReader;

fn project_dir_name(project_dir: &Path) -> String {
    // ~/.claude/projects/<path with - for / and leading ->
    let s = project_dir.to_string_lossy().replace('/', "-");
    s
}

impl SessionReader for ClaudeCodeReader {
    fn name(&self) -> &'static str {
        "claude-code"
    }

    fn latest(&self, project_dir: &Path) -> Option<SessionInfo> {
        let root = dirs_home()?
            .join(".claude")
            .join("projects")
            .join(project_dir_name(project_dir));
        if !root.is_dir() {
            return None;
        }
        let mut best: Option<(PathBuf, i64)> = None;
        for entry in std::fs::read_dir(&root).ok()? {
            let p = entry.ok()?.path();
            if p.extension().map(|e| e == "jsonl").unwrap_or(false) {
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
        }
        let (path, updated) = best?;
        parse(&path, project_dir, updated)
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn parse(path: &Path, project_dir: &Path, updated: i64) -> Option<SessionInfo> {
    let mut first_user: Option<String> = None;
    let mut last_user: Option<String> = None;
    let mut last_assistant: Option<String> = None;
    let mut errors: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    let mut commands: Vec<String> = Vec::new();

    crate::_read_jsonl(path, |v| {
        match v.get("type").and_then(|t| t.as_str()) {
            Some("user") => {
                let text = v
                    .get("message")
                    .map(|m| content_text(m.get("content").unwrap_or(&serde_json::Value::Null)))
                    .unwrap_or_default();
                if !text.is_empty() {
                    if first_user.is_none() {
                        first_user = Some(excerpt(&text, 300));
                    }
                    last_user = Some(excerpt(&text, 300));
                }
            }
            Some("assistant") => {
                let text = v
                    .get("message")
                    .map(|m| content_text(m.get("content").unwrap_or(&serde_json::Value::Null)))
                    .unwrap_or_default();
                if !text.is_empty() {
                    last_assistant = Some(excerpt(&text, 400));
                }
                // tool uses inside content blocks
                if let Some(blocks) = v
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_array())
                {
                    for b in blocks {
                        if b.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                            let name = b.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            let input = b.get("input").cloned().unwrap_or(serde_json::Value::Null);
                            if name.contains("Bash") || name == "bash" || name == "terminal" {
                                if let Some(cmd) = input.get("command").and_then(|c| c.as_str()) {
                                    commands.push(excerpt(cmd, 120));
                                }
                            } else if name.contains("Edit")
                                || name.contains("Write")
                                || name == "file_edit"
                                || name == "write_file"
                            {
                                if let Some(f) = input.get("file_path").and_then(|f| f.as_str()) {
                                    files.push(f.to_string());
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        if v.get("error")
            .and_then(|e| e.as_str())
            .map(|e| !e.is_empty())
            .unwrap_or(false)
        {
            let e = v.get("error").and_then(|e| e.as_str()).unwrap_or("");
            errors.push(excerpt(e, 200));
        }
    })
    .ok();

    Some(SessionInfo {
        agent: "claude-code".into(),
        session_id: path.file_stem()?.to_string_lossy().into(),
        project_dir: project_dir.to_path_buf(),
        updated_unix: updated,
        first_user_message: first_user,
        last_user_message: last_user,
        last_assistant_excerpt: last_assistant,
        errors,
        files_touched: dedup(files),
        commands,
    })
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

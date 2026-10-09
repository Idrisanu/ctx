use ctx_core::ProjectContext;

const NOTE_BLOCK_START: &str = "<!-- CTX:START -->";
const NOTE_BLOCK_END: &str = "<!-- CTX:END -->";

pub(crate) fn inject_cooperative_note(root: &std::path::Path) {
    // Always write AGENTS.md (canonical default); also honor the agent's
    // own file when the project already uses it. Never rename files.
    let mut targets = vec![root.join("AGENTS.md")];
    for name in ["CLAUDE.md", "GEMINI.md"] {
        let p = root.join(name);
        if p.exists() {
            targets.push(p);
        }
    }
    for path in targets {
        inject_note_file(&path);
    }
}

pub(crate) fn inject_note_file(path: &std::path::Path) {
    let block = format!(
        "{}\n## For AI coding agents (managed by CTX)\n\nMaintain `.ctx/handoffs/current.md` while working. Update it after every milestone, every batch of tool calls, and before ending a turn — not only at the end. Include:\n\n- Goal\n- Completed\n- Current work\n- Last command run (and its result)\n- Last error, if any\n- Next action\n- Blockers\n\nCTX reads this file to build the handoff when the session ends (token limit, crash, or tool switch). Keep it short and factual.\n\nWhen you establish a lasting pattern or make an architectural choice, also record it with `ctx decide \"...\" --reason \"...\"` so the next agent cannot silently contradict it.\n{}\n",
        NOTE_BLOCK_START, NOTE_BLOCK_END
    );
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let updated = if let (Some(s), Some(e)) = (
        existing.find(NOTE_BLOCK_START),
        existing.find(NOTE_BLOCK_END),
    ) {
        let end = e + NOTE_BLOCK_END.len();
        format!("{}{}{}", &existing[..s], block, &existing[end..])
    } else if existing.is_empty() {
        block
    } else {
        format!("{}\n\n{}", existing.trim_end(), block)
    };
    if let Err(e) = std::fs::write(path, updated) {
        eprintln!("warning: could not update {}: {}", path.display(), e);
    } else {
        println!(
            "{} updated with CTX note instructions.",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
    }
}

/// Overlay the AI cooperative note onto state for display purposes.
/// Note wins for intent fields (it's the AI's freshest own statement);
/// git remains the source of truth for files.
pub(crate) fn apply_note(state: &mut ProjectContext, root: &std::path::Path) {
    let path = root.join(".ctx/handoffs/current.md");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return;
    };
    let mut goal = None;
    let mut completed = Vec::new();
    let mut current = Vec::new();
    let mut next = None;
    let mut blockers = Vec::new();
    let mut section = "";
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with("- Goal:") {
            goal = clean_note_value(t.strip_prefix("- Goal:").unwrap_or(""));
        } else if t.starts_with("- Completed:") {
            section = "completed";
            let v = clean_note_value(t.strip_prefix("- Completed:").unwrap_or(""));
            if let Some(v) = v {
                completed.push(v);
            }
        } else if t.starts_with("- Current work:") {
            section = "current";
            let v = clean_note_value(t.strip_prefix("- Current work:").unwrap_or(""));
            if let Some(v) = v {
                current.push(v);
            }
        } else if t.starts_with("- Next action:") {
            section = "next";
            next = clean_note_value(t.strip_prefix("- Next action:").unwrap_or(""));
        } else if t.starts_with("- Blockers:") {
            section = "blockers";
            let v = clean_note_value(t.strip_prefix("- Blockers:").unwrap_or(""));
            if let Some(v) = v {
                blockers.push(v);
            }
        } else if t.starts_with("- ") && !section.is_empty() {
            let v = t[2..].trim().to_string();
            if v.is_empty() {
                continue;
            }
            match section {
                "completed" => completed.push(v),
                "current" => current.push(v),
                "blockers" => blockers.push(v),
                _ => {}
            }
        } else if t.starts_with("- Constraints:") {
            section = "constraints";
            let v = clean_note_value(t.strip_prefix("- Constraints:").unwrap_or(""));
            if let Some(v) = v {
                state.constraints.push(v);
            }
        } else if t.is_empty() || t.starts_with('#') {
            // keep section sticky across blank/header lines
        } else {
            section = "";
        }
    }
    if let Some(g) = goal {
        if state.objective.is_none() || state.objective.as_deref() == Some("(unset)") {
            state.objective = Some(g);
        } else {
            // note is fresher: let it win but keep state fallback in case note is empty
            state.objective = Some(g);
        }
    }
    if !completed.is_empty() {
        for c in completed {
            if !state.completed.contains(&c) {
                state.completed.push(c);
            }
        }
    }
    if !current.is_empty() {
        state.current_work = current;
    }
    if let Some(n) = next {
        state.next_action = Some(n);
    }
    if !blockers.is_empty() {
        for b in blockers {
            let entry = format!("BLOCKER: {}", b);
            if !state.errors.contains(&entry) {
                state.errors.push(entry);
            }
        }
    }
}

pub(crate) fn clean_note_value(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("none") || t == "-" || t == "N/A" {
        None
    } else {
        Some(t.to_string())
    }
}

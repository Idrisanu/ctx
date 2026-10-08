use ctx_core::ProjectContext;
use ctx_git::GitInfo;

pub trait Adapter {
    fn name(&self) -> &'static str;
    fn render(&self, state: &ProjectContext, git: &GitInfo) -> String;
}

pub struct GenericAdapter;
pub struct OpenCodeAdapter;

/// Same canonical content, formatted for the tool about to read it.
/// The adapter only changes how ctx talks — it never renames or moves
/// project files (AGENTS.md / CLAUDE.md / GEMINI.md stay as they are).
pub struct FlavoredAdapter {
    id: &'static str,
    label: &'static str,
    doc_pointer: &'static str,
}

pub fn get(name: &str) -> Option<Box<dyn Adapter>> {
    match name {
        "generic" | "markdown" => Some(Box::new(GenericAdapter)),
        "opencode" => Some(Box::new(OpenCodeAdapter)),
        "claude" | "claude-code" => Some(Box::new(FlavoredAdapter {
            id: "claude",
            label: "Claude Code",
            doc_pointer: "CLAUDE.md (or AGENTS.md) in the repo root",
        })),
        "gemini" | "gemini-cli" => Some(Box::new(FlavoredAdapter {
            id: "gemini",
            label: "Gemini CLI",
            doc_pointer: "GEMINI.md (or AGENTS.md) in the repo root",
        })),
        "codex" => Some(Box::new(FlavoredAdapter {
            id: "codex",
            label: "Codex",
            doc_pointer: "AGENTS.md in the repo root",
        })),
        "copilot" | "vscode" => Some(Box::new(FlavoredAdapter {
            id: "copilot",
            label: "VS Code / Copilot",
            doc_pointer: "AGENTS.md in the repo root",
        })),
        _ => None,
    }
}

/// All adapter names `ctx resume` / `ctx handoff` accept.
pub fn names() -> Vec<&'static str> {
    vec![
        "generic", "opencode", "claude", "gemini", "codex", "copilot",
    ]
}

impl Adapter for GenericAdapter {
    fn name(&self) -> &'static str {
        "generic"
    }
    fn render(&self, state: &ProjectContext, git: &GitInfo) -> String {
        render_handoff(state, git)
    }
}

impl Adapter for OpenCodeAdapter {
    fn name(&self) -> &'static str {
        "opencode"
    }
    fn render(&self, state: &ProjectContext, git: &GitInfo) -> String {
        let mut out = String::new();
        out.push_str("<!-- CTX: project context for OpenCode -->\n\n");
        out.push_str("# Project Context (from CTX)\n\n");
        out.push_str(&format!("**Project:** {}\n\n", state.project));
        if let Some(o) = &state.objective {
            out.push_str(&format!("**Objective:** {}\n\n", o));
        }
        out.push_str(&format!("**Status:** {}\n\n", state.status));
        if !state.completed.is_empty() {
            out.push_str("## Completed\n");
            for c in &state.completed {
                out.push_str(&format!("- {}\n", c));
            }
            out.push('\n');
        }
        if !state.current_work.is_empty() {
            out.push_str("## Current work\n");
            for c in &state.current_work {
                out.push_str(&format!("- {}\n", c));
            }
            out.push('\n');
        }
        if !state.constraints.is_empty() {
            out.push_str("## Constraints\n");
            for c in &state.constraints {
                out.push_str(&format!("- {}\n", c));
            }
            out.push('\n');
        }
        if !state.decisions.is_empty() {
            out.push_str("## Decisions\n");
            for d in state.decisions.iter().filter(|d| d.active()) {
                out.push_str(&format!("- **{}**", d.decision));
                if let Some(r) = &d.reason {
                    out.push_str(&format!(" — {}", r));
                }
                out.push('\n');
            }
            out.push('\n');
        }
        let files = if git.dirty_files.is_empty() {
            &state.files_changed
        } else {
            &git.dirty_files
        };
        if !files.is_empty() {
            out.push_str("## Recently changed files\n");
            for f in files {
                out.push_str(&format!("- {}\n", f));
            }
            out.push('\n');
        }
        if let Some(next) = &state.next_action {
            out.push_str(&format!("## Next action\n{}\n", next));
        }
        out
    }
}

impl Adapter for FlavoredAdapter {
    fn name(&self) -> &'static str {
        self.id
    }
    fn render(&self, state: &ProjectContext, git: &GitInfo) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "<!-- CTX: project context for {} — also read {} for project rules -->\n\n",
            self.label, self.doc_pointer
        ));
        out.push_str("# Project Context (from CTX)\n\n");
        out.push_str(&format!("**Project:** {}\n\n", state.project));
        if let Some(o) = &state.objective {
            out.push_str(&format!("**Objective:** {}\n\n", o));
        }
        out.push_str(&format!("**Status:** {}\n\n", state.status));
        if !state.completed.is_empty() {
            out.push_str("## Completed\n");
            for c in &state.completed {
                out.push_str(&format!("- {}\n", c));
            }
            out.push('\n');
        }
        if !state.current_work.is_empty() {
            out.push_str("## Current work\n");
            for c in &state.current_work {
                out.push_str(&format!("- {}\n", c));
            }
            out.push('\n');
        }
        if !state.constraints.is_empty() {
            out.push_str("## Constraints\n");
            for c in &state.constraints {
                out.push_str(&format!("- {}\n", c));
            }
            out.push('\n');
        }
        if !state.decisions.is_empty() {
            out.push_str("## Decisions (do not contradict without asking)\n");
            for d in state.decisions.iter().filter(|d| d.active()) {
                out.push_str(&format!("- **{}**", d.decision));
                if let Some(r) = &d.reason {
                    out.push_str(&format!(" — {}", r));
                }
                out.push('\n');
            }
            out.push('\n');
        }
        let files = if git.dirty_files.is_empty() {
            &state.files_changed
        } else {
            &git.dirty_files
        };
        if !files.is_empty() {
            out.push_str("## Recently changed files\n");
            for f in files {
                out.push_str(&format!("- {}\n", f));
            }
            out.push('\n');
        }
        if !state.errors.is_empty() {
            out.push_str("## Known errors / blockers\n");
            for e in &state.errors {
                out.push_str(&format!("- {}\n", e));
            }
            out.push('\n');
        }
        if let Some(next) = &state.next_action {
            out.push_str(&format!("## Next action\n{}\n", next));
        } else {
            out.push_str("## Next action\n(continue from Current work above)\n");
        }
        out
    }
}

pub fn render_handoff(state: &ProjectContext, git: &GitInfo) -> String {
    let mut out = String::new();
    out.push_str("CTX HANDOFF\n\n");
    out.push_str(&format!("Project:\n{}\n\n", state.project));
    out.push_str(&format!(
        "Task:\n{}\n\n",
        state.objective.as_deref().unwrap_or("(unset)")
    ));
    out.push_str("Completed:\n");
    for c in &state.completed {
        out.push_str(&format!("✓ {}\n", c));
    }
    if state.completed.is_empty() {
        out.push_str("(none recorded)\n");
    }
    out.push('\n');
    out.push_str("Current:\n");
    for c in &state.current_work {
        out.push_str(&format!("→ {}\n", c));
    }
    if state.current_work.is_empty() {
        out.push_str("(none)\n");
    }
    out.push('\n');
    let files = if git.dirty_files.is_empty() {
        &state.files_changed
    } else {
        &git.dirty_files
    };
    out.push_str("Files:\n");
    for f in files {
        out.push_str(&format!("{}\n", f));
    }
    if files.is_empty() {
        out.push_str("(none)\n");
    }
    out.push('\n');
    out.push_str("Constraints:\n");
    for c in &state.constraints {
        out.push_str(&format!("- {}\n", c));
    }
    if state.constraints.is_empty() {
        out.push_str("(none)\n");
    }
    out.push('\n');
    out.push_str(&format!(
        "Next action:\n{}\n",
        state.next_action.as_deref().unwrap_or("(unset)")
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opencode_render_contains_project() {
        let mut s = ProjectContext::default();
        s.project = "demo".into();
        let g = GitInfo::default();
        let out = OpenCodeAdapter.render(&s, &g);
        assert!(out.contains("demo"));
    }
    #[test]
    fn all_adapters_render() {
        let mut s = ProjectContext::default();
        s.project = "demo".into();
        s.decisions.push(ctx_core::Decision {
            id: "CTX-1".into(),
            decision: "PostgreSQL".into(),
            reason: None,
            recorded_at: None,
            superseded_by: None,
            conflicts_with: None,
        });
        let g = GitInfo::default();
        for n in names() {
            let a = get(n).unwrap();
            let out = a.render(&s, &g);
            assert!(out.contains("demo"), "adapter {}", n);
            if n != "generic" && n != "opencode" {
                assert!(out.contains("PostgreSQL"), "adapter {}", n);
            }
        }
    }
}

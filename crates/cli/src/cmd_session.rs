use super::{open_ctx, project_root};
use crate::note::apply_note;
use anyhow::Result;
use ctx_core::ProjectContext;
use ctx_storage::CtxDir;

pub(crate) fn cmd_handoff(agent: Option<String>) -> Result<()> {
    let (ctx, root) = open_ctx()?;
    let mut state = ctx.load_state()?;
    apply_note(&mut state, &root);
    let git = ctx_git::info(&root);

    // Optional per-agent rendering. This only changes how ctx talks —
    // it never renames project files or agent folders.
    if let Some(name) = agent.as_deref() {
        let adapter = ctx_adapters::get(name).ok_or_else(|| {
            anyhow::anyhow!(
                "unknown agent '{}' (available: {})",
                name,
                ctx_adapters::names().join(", ")
            )
        })?;
        println!("{}", adapter.render(&state, &git));
        return Ok(());
    }

    println!("CTX HANDOFF");
    println!();
    println!("Project:\n{}\n", state.project);
    println!(
        "Task:\n{}\n",
        state.objective.as_deref().unwrap_or("(unset)")
    );
    println!("Completed:");
    for c in &state.completed {
        println!("✓ {}", c);
    }
    if state.completed.is_empty() {
        println!("(none recorded)");
    }
    println!();
    println!("Current:");
    for c in &state.current_work {
        println!("→ {}", c);
    }
    if state.current_work.is_empty() {
        println!("(none)");
    }
    println!();
    println!("Files:");
    let files = if git.dirty_files.is_empty() {
        &state.files_changed
    } else {
        &git.dirty_files
    };
    for f in files {
        println!("{}", f);
    }
    if files.is_empty() {
        println!("(none)");
    }
    println!();
    println!("Constraints:");
    for c in &state.constraints {
        println!("- {}", c);
    }
    if state.constraints.is_empty() {
        println!("(none)");
    }
    println!();
    println!(
        "Next action:\n{}",
        state.next_action.as_deref().unwrap_or("(unset)")
    );
    println!();
    println!("Instructions:");
    let cwd = project_root();
    let inst = ctx_context::instructions_for(&root, &cwd);
    if inst.is_empty() {
        println!("(none)");
    } else {
        for p in &inst {
            println!("{}", p.strip_prefix(&root).unwrap_or(p).display());
        }
    }
    Ok(())
}

pub(crate) fn cmd_resume(agent: Option<String>) -> Result<()> {
    let (ctx, root) = open_ctx()?;
    let mut state = ctx.load_state()?;
    apply_note(&mut state, &root);
    let git = ctx_git::info(&root);
    let primary = ctx.load_config().ok().and_then(|c| c.primary_agent);
    let agent_name = agent.as_deref().or(primary.as_deref()).unwrap_or("generic");
    let adapter = ctx_adapters::get(agent_name).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown agent '{}' (available: {})",
            agent_name,
            ctx_adapters::names().join(", ")
        )
    })?;
    let mut rendered = adapter.render(&state, &git);
    // Uncommitted frontier: the exact place mid-milestone work stopped.
    let cps = ctx.load_checkpoints().unwrap_or_default();
    let (added, removed) = working_diff(&cps, &git);
    if !added.is_empty() || !removed.is_empty() {
        rendered.push_str("\n## Uncommitted changes since last checkpoint\n");
        match cps.last() {
            Some(cp) => rendered.push_str(&format!("Baseline: {}\n", cp.id)),
            None => rendered.push_str("Baseline: (none — everything below is uncommitted)\n"),
        }
        for f in &added {
            rendered.push_str(&format!("+ {}\n", f));
        }
        for f in &removed {
            rendered.push_str(&format!("- {}\n", f));
        }
    }
    // provenance: say where each part came from
    rendered.push_str("\n---\nSources: project/git state");
    if state.ingested.is_some() {
        rendered.push_str(" + ingested AI session");
    }
    if root.join(".ctx/handoffs/current.md").exists() {
        rendered.push_str(" + AI cooperative note");
    }
    rendered.push('\n');
    let conflicts = ctx_core::live_conflicts(&state);
    if !conflicts.is_empty() {
        rendered.push_str("\n## ⚠ Possible decision conflicts (resolve before continuing)\n");
        for d in &conflicts {
            let other = d
                .conflicts_with
                .as_deref()
                .and_then(|c| state.decisions.iter().find(|x| x.id == c));
            rendered.push_str(&format!(
                "- {} says: {}\n  but {} says: {}\n",
                d.id,
                d.decision,
                other.map(|o| o.id.as_str()).unwrap_or("?"),
                other.map(|o| o.decision.as_str()).unwrap_or("?")
            ));
        }
    }
    if let Some(ing) = &state.ingested {
        rendered.push_str(&format!(
            "\n## Last AI session ({})\nGoal: {}\n",
            ing.agent,
            ing.goal.as_deref().unwrap_or("(unknown)")
        ));
        if let Some(g) = &ing.last_user_message {
            rendered.push_str(&format!("Last request: {}\n", g));
        }
        if let Some(a) = &ing.last_assistant_excerpt {
            rendered.push_str(&format!("Last assistant message: {}\n", a));
        }
        if let Some(p) = &ing.progress {
            rendered.push_str(&format!("Latest progress report: {}\n", p));
        }
        if let Some(t) = ing.tokens_total {
            rendered.push_str(&format!("Session tokens used: ~{}\n", t));
        }
        if !ing.errors.is_empty() {
            rendered.push_str("Errors:\n");
            for e in &ing.errors {
                rendered.push_str(&format!("- {}\n", e));
            }
        }
        if !ing.commands.is_empty() {
            rendered.push_str("Recent commands (most recent last):\n");
            for c in ing.commands.iter().rev().take(5).rev() {
                rendered.push_str(&format!("- `{}`\n", c));
            }
        }
    }
    if !state.note_commands.is_empty() {
        rendered.push_str("Commands from AI note:\n");
        for c in &state.note_commands {
            rendered.push_str(&format!("- `{}`\n", c));
        }
    }
    let cwd = project_root();
    let merged = ctx_context::merged_instructions(&root, &cwd);
    if !merged.is_empty() {
        rendered.push_str("\n## Project instructions\n\n");
        rendered.push_str(&merged);
    }
    let discovered = ctx_context::discover(&root);
    if !discovered.planning.is_empty() || !discovered.readmes.is_empty() {
        rendered.push_str("\n## Project documents (read these for full context)\n");
        for p in &discovered.planning {
            rendered.push_str(&format!(
                "- {}\n",
                p.strip_prefix(&root).unwrap_or(p).display()
            ));
        }
        for p in &discovered.readmes {
            rendered.push_str(&format!(
                "- {}\n",
                p.strip_prefix(&root).unwrap_or(p).display()
            ));
        }
    }
    let dir = ctx.root.join("handoffs");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.md", adapter.name()));
    std::fs::write(&path, &rendered)?;
    println!(
        "Context prepared for '{}':\n{}",
        adapter.name(),
        path.display()
    );
    if !git.dirty_files.is_empty() {
        println!(
            "⚠ {} uncommitted file(s) — commit before switching so the handoff pins them (`ctx commit -m \"...\"` or git commit).",
            git.dirty_files.len()
        );
    }
    Ok(())
}

pub(crate) fn cmd_recover() -> Result<()> {
    let (ctx, root) = open_ctx()?;
    let mut state = ctx.load_state()?;
    apply_note(&mut state, &root);
    let git = ctx_git::info(&root);
    let cps = ctx.load_checkpoints()?;

    println!("Last session");
    println!("────────────────────\n");
    match cps.last() {
        Some(cp) => println!(
            "Checkpoint: {} ({})\nBranch: {}\nCommit: {}\nFiles changed: {}\n",
            cp.id,
            cp.created_at,
            cp.branch.as_deref().unwrap_or("-"),
            cp.git_commit.as_deref().unwrap_or("-"),
            cp.files_changed
        ),
        None => println!("No checkpoints recorded yet.\n"),
    }
    println!(
        "Task:\n{}\n\nStatus: {}\n",
        state.objective.as_deref().unwrap_or("(unset)"),
        state.status
    );
    println!("Last changed files:");
    let files = if git.dirty_files.is_empty() {
        &state.files_changed
    } else {
        &git.dirty_files
    };
    for f in files {
        println!("{}", f);
    }
    if files.is_empty() {
        println!("(none)");
    }
    println!(
        "\nNext action:\n{}",
        state.next_action.as_deref().unwrap_or("(unset)")
    );
    Ok(())
}

pub(crate) fn cmd_diff() -> Result<()> {
    let (ctx, root) = open_ctx()?;
    let git = ctx_git::info(&root);
    let cps = ctx.load_checkpoints()?;
    print_diff(&cps, &git);
    Ok(())
}

/// Working tree vs last checkpoint, shared by `ctx diff` and `ctx resume`.
pub(crate) fn working_diff(
    cps: &[ctx_core::Checkpoint],
    git: &ctx_git::GitInfo,
) -> (Vec<String>, Vec<String>) {
    let last: Vec<String> = cps.last().map(|c| c.files.clone()).unwrap_or_default();
    let now: Vec<String> = git.dirty_files.clone();
    let added: Vec<String> = now.iter().filter(|f| !last.contains(f)).cloned().collect();
    let removed: Vec<String> = last.iter().filter(|f| !now.contains(f)).cloned().collect();
    (added, removed)
}

fn print_diff(cps: &[ctx_core::Checkpoint], git: &ctx_git::GitInfo) {
    let (added, removed) = working_diff(cps, git);

    println!("Context diff (vs last checkpoint)");
    match cps.last() {
        Some(cp) => println!("Baseline: {}", cp.id),
        None => println!("Baseline: (none)"),
    }
    println!(
        "Branch: {} -> {}\n",
        cps.last()
            .and_then(|c| c.branch.clone())
            .as_deref()
            .unwrap_or("-"),
        git.branch.as_deref().unwrap_or("-")
    );
    if !added.is_empty() {
        println!("New changes:");
        for f in &added {
            println!("  + {}", f);
        }
    }
    if !removed.is_empty() {
        println!("Resolved/reverted:");
        for f in &removed {
            println!("  - {}", f);
        }
    }
    if added.is_empty() && removed.is_empty() {
        println!("No working-tree changes since last checkpoint.");
    }
}

pub(crate) fn cmd_ingest(from: Option<String>) -> Result<()> {
    let (ctx, root) = open_ctx()?;
    let mut state = ctx.load_state()?;

    let session = match &from {
        Some(path) => parse_generic_jsonl(std::path::Path::new(path), &root),
        None => ctx_ingest::latest_for(&root),
    };

    let Some(session) = session else {
        println!("No AI session found for this project.");
        return Ok(());
    };

    if !merge_session(&ctx, &mut state, &session)? {
        println!("Already up to date with this session.");
        return Ok(());
    }
    let compressed = ctx_ingest::compress(&session);
    println!("Ingested {} ({})", compressed.agent, compressed.source);
    if let Some(g) = &compressed.goal {
        println!("Goal: {}", g);
    }
    if let Some(e) = &compressed.last_assistant_excerpt {
        println!("Last message: {}", e);
    }
    if !compressed.errors.is_empty() {
        println!("Errors: {}", compressed.errors.len());
    }
    Ok(())
}

pub(crate) fn parse_generic_jsonl(
    path: &std::path::Path,
    root: &std::path::Path,
) -> Option<ctx_ingest::SessionInfo> {
    let mut first_user = None;
    let mut last_user = None;
    let mut last_assistant = None;
    ctx_ingest::_read_jsonl(path, |v| {
        let t = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let text = v
            .get("text")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .or_else(|| v.get("content").map(ctx_ingest::content_text))
            .or_else(|| {
                v.get("message").map(|m| {
                    ctx_ingest::content_text(m.get("content").unwrap_or(&serde_json::Value::Null))
                })
            })
            .unwrap_or_default();
        if text.is_empty() {
            return;
        }
        match t {
            "user" => {
                if first_user.is_none() {
                    first_user = Some(ctx_ingest::excerpt(&text, 300));
                }
                last_user = Some(ctx_ingest::excerpt(&text, 300));
            }
            "assistant" => last_assistant = Some(ctx_ingest::excerpt(&text, 400)),
            _ => {}
        }
    })
    .ok()?;
    Some(ctx_ingest::SessionInfo {
        agent: "generic".into(),
        session_id: path.file_stem()?.to_string_lossy().into(),
        project_dir: root.to_path_buf(),
        updated_unix: std::fs::metadata(path)
            .ok()?
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs() as i64,
        first_user_message: first_user,
        last_user_message: last_user,
        last_assistant_excerpt: last_assistant,
        ..Default::default()
    })
}

pub(crate) fn merge_session(
    ctx: &CtxDir,
    state: &mut ProjectContext,
    session: &ctx_ingest::SessionInfo,
) -> Result<bool> {
    // skip when we already have this exact session version
    if let Some(ing) = &state.ingested {
        if ing.source == ctx_ingest::compress(session).source
            && ing.updated_unix >= session.updated_unix
        {
            return Ok(false);
        }
    }
    let compressed = ctx_ingest::compress(session);
    if state.objective.is_none() {
        state.objective = compressed.goal.clone();
    }
    for e in &compressed.errors {
        if !state.errors.contains(e) {
            state.errors.push(e.clone());
        }
    }
    for f in &compressed.files_touched {
        if !state.files_changed.contains(f) {
            state.files_changed.push(f.clone());
        }
    }
    state.ingested = Some(ctx_core::Ingested {
        agent: compressed.agent.clone(),
        goal: compressed.goal.clone(),
        last_user_message: compressed.last_user_message.clone(),
        last_assistant_excerpt: compressed.last_assistant_excerpt.clone(),
        progress: compressed.progress.clone(),
        tokens_total: compressed.tokens_total,
        errors: compressed.errors.clone(),
        files_touched: compressed.files_touched.clone(),
        commands: compressed.commands.clone(),
        source: compressed.source.clone(),
        updated_unix: session.updated_unix,
    });
    ctx.save_state(state)?;
    Ok(true)
}

pub(crate) fn cmd_monitor(interval: u64, once: bool) -> Result<()> {
    let (ctx, root) = open_ctx()?;
    println!(
        "Monitoring AI sessions for {} (every {}s — Ctrl-C to stop)",
        root.display(),
        interval
    );
    loop {
        let mut state = ctx.load_state()?;
        for reader in ctx_ingest::registry() {
            // cheap mtime probe first; skip full parse when nothing changed
            let mtime = match reader.latest_mtime(&root) {
                Some(m) => m,
                None => continue,
            };
            let fresh = match &state.ingested {
                Some(ing) => ing.updated_unix < mtime,
                None => true,
            };
            if !fresh {
                continue;
            }
            if let Some(session) = reader.latest(&root) {
                if merge_session(&ctx, &mut state, &session)? {
                    println!(
                        "ctx: updated from {} session {}",
                        session.agent, session.session_id
                    );
                }
            }
        }
        if once {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(interval.max(5)));
    }
    Ok(())
}

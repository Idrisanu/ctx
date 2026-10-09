use super::{detect_project_name, open_ctx, project_root};
use crate::note::{agent_instruction_file, apply_note, init_agent_names, inject_cooperative_note};
use anyhow::Result;
use ctx_core::{Checkpoint, Config, EnvironmentState, ProjectContext};
use ctx_storage::CtxDir;
use std::path::PathBuf;

pub(crate) fn cmd_init(yes: bool, agent: Option<String>) -> Result<()> {
    let root = project_root();
    let ctx = CtxDir::at(&root);
    let already = ctx.exists();
    ctx.init()?;

    // Declared agents: validate, resolve their canonical files.
    let mut extra_files: Vec<String> = Vec::new();
    let mut primary_agent: Option<String> = None;
    if let Some(list) = agent.as_deref() {
        for name in list
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
        {
            if !init_agent_names().contains(&name.as_str()) {
                anyhow::bail!(
                    "unknown agent '{}' (valid: {})",
                    name,
                    init_agent_names().join(", ")
                );
            }
            if primary_agent.is_none() {
                primary_agent = Some(name.clone());
            }
            if let Some(file) = agent_instruction_file(&name) {
                if !extra_files.contains(&file.to_string()) {
                    extra_files.push(file.to_string());
                }
            }
        }
    }

    // --- git setup before we probe state ---
    let git_installed = ctx_git::git_available();
    let mut git = ctx_git::info(&root);
    if !git.is_repo
        && git_installed
        && !already
        && (yes
            || prompt_yes(
                "Git is installed but this folder is not a repository. Run `git init` now? [y/N] ",
            ))
    {
        let status = std::process::Command::new("git")
            .arg("init")
            .current_dir(&root)
            .status();
        if status.map(|s| s.success()).unwrap_or(false) {
            println!("Initialized empty git repository.");
            git = ctx_git::info(&root);
        } else {
            eprintln!("⚠ git init failed — continuing without git.");
        }
    }

    let docs = ctx_context::discover(&root);
    let env = ctx_env::detect(&root);

    // On re-init, preserve existing state + config; otherwise create fresh.
    let mut state = if already && ctx.state_path().exists() {
        ctx.load_state().unwrap_or_default()
    } else {
        ProjectContext {
            project: detect_project_name(&root),
            status: "initialized".into(),
            ..Default::default()
        }
    };

    state.environment = EnvironmentState {
        runtimes: vec![],
        package_managers: vec![],
        docker: Some(env.docker),
        env_vars_required: ctx_env::required_env_vars(&root),
        env_vars_missing: vec![],
    };
    if let Some(node) = &env.node {
        state.environment.runtimes.push(format!("node {}", node));
    }
    if let Some(p) = &env.python {
        state.environment.runtimes.push(format!("python {}", p));
    }
    if let Some(r) = &env.rust {
        state.environment.runtimes.push(r.clone());
    }
    if let Some(g) = &env.go {
        state.environment.runtimes.push(g.clone());
    }
    if env.pnpm.is_some() {
        state.environment.package_managers.push("pnpm".into());
    }
    if env.npm.is_some() {
        state.environment.package_managers.push("npm".into());
    }
    state.files_changed = git.dirty_files.clone();

    let mut config = if already && ctx.config_path().exists() {
        ctx.load_config().unwrap_or(Config {
            project_name: state.project.clone(),
            checkpoint_counter: 0,
            created_at: Some(chrono_now()),
            primary_agent: None,
        })
    } else {
        Config {
            project_name: state.project.clone(),
            checkpoint_counter: 0,
            created_at: Some(chrono_now()),
            primary_agent: None,
        }
    };
    if primary_agent.is_some() {
        config.primary_agent = primary_agent;
    }
    ctx.save_config(&config)?;
    ctx.save_state(&state)?;
    if !ctx.checkpoints_path().exists() {
        ctx.save_checkpoints(&[])?;
    }

    println!(
        "{} CTX in {}",
        if already { "Refreshed" } else { "Initialized" },
        ctx.root.display()
    );
    println!("Project: {}", state.project);
    if git.is_repo {
        println!(
            "Git: detected ({})",
            git.branch.as_deref().unwrap_or("detached")
        );
    } else if git_installed {
        println!("Git: installed, but this folder is not a repository.");
        println!("      Run `git init` then `ctx init` to enable auto-checkpoints.");
    } else {
        println!("Git: not installed. CTX works without it, but checkpoints and");
        println!("      change tracking are limited. Install with:");
        println!("        sudo apt install git   # Debian/Ubuntu");
        println!("        brew install git       # macOS");
        println!("        winget install Git.Git # Windows");
    }
    println!("Instruction files found: {}", docs.instructions.len());
    println!("Runtimes: {:?}", state.environment.runtimes);

    inject_cooperative_note(&root, &extra_files);
    install_git_hook(&root);
    Ok(())
}

pub(crate) fn prompt_yes(question: &str) -> bool {
    use std::io::Write;
    print!("{}", question);
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).ok();
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
}

pub(crate) fn install_git_hook(root: &std::path::Path) {
    let hooks = root.join(".git").join("hooks");
    if !hooks.is_dir() {
        return;
    }
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("ctx"));
    let hook = format!(
        "#!/bin/sh\n\"{}\" checkpoint-auto >/dev/null 2>&1 || true\n",
        exe.display()
    );
    let path = hooks.join("post-commit");
    if std::fs::write(&path, hook).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
        }
        println!("git post-commit hook installed (auto-checkpoint).");
    }
}

pub(crate) fn chrono_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub(crate) fn cmd_status() -> Result<()> {
    let (ctx, root) = open_ctx()?;
    let mut state = ctx.load_state()?;
    apply_note(&mut state, &root);
    let git = ctx_git::info(&root);
    let cps = ctx.load_checkpoints()?;

    println!("CTX");
    println!("──────────────────────────────");
    println!("Project       {}", state.project);
    println!("Status        {}", state.status);
    if let Some(t) = &state.objective {
        println!("Objective     {}", t);
    }
    println!("Git           {}", git.branch.as_deref().unwrap_or("none"));
    println!("Changes       {} files", git.uncommitted_count);
    if !git.dirty_files.is_empty() {
        println!("⚠ Uncommitted work — commit before switching agents (`ctx commit -m \"...\"`)");
    }
    println!("Context       Unknown (no live agent data)");
    match cps.last() {
        Some(cp) => println!("Checkpoint    {} ({})", cp.id, cp.created_at),
        None => println!("Checkpoint    none"),
    }
    if let Some(next) = &state.next_action {
        println!("Next:\n{}", next);
    }
    println!();
    let note = root.join(".ctx/handoffs/current.md");
    if note.exists() {
        if let Ok(meta) = std::fs::metadata(&note) {
            if let Ok(mtime) = meta.modified() {
                let age = mtime.elapsed().unwrap_or_default().as_secs();
                if age > 24 * 3600 {
                    println!("⚠ AI note is {}h old — may be stale", age / 3600);
                } else {
                    println!("AI note: present ({}h old)", age / 3600);
                }
            }
        }
    } else {
        println!("AI note: not created yet (agent may not follow AGENTS.md instructions)");
    }
    if let Some(ing) = &state.ingested {
        println!("Last ingested: {} ({})", ing.agent, ing.source);
    }
    let conflicts = ctx_core::live_conflicts(&state);
    if !conflicts.is_empty() {
        println!(
            "⚠ {} possible decision conflict(s) — `ctx inspect decisions`",
            conflicts.len()
        );
    }
    Ok(())
}

pub(crate) fn cmd_doctor() -> Result<()> {
    let root = project_root();
    let env = ctx_env::detect(&root);
    let git = ctx_git::info(&root);

    println!("CTX Environment Check");
    println!();
    println!("{} Git", mark(env.git));
    println!(
        "{} Repository {}",
        mark(git.is_repo),
        if git.is_repo { "" } else { "(not a git repo)" }
    );
    print_opt("Node.js", &env.node);
    print_opt("pnpm", &env.pnpm);
    print_opt("npm", &env.npm);
    print_opt("Python", &env.python);
    print_opt("Rust", &env.rust);
    print_opt("Go", &env.go);
    println!("{} Docker", mark(env.docker));
    println!("{} Docker Compose", mark(env.docker_compose));
    if env.databases.is_empty() {
        println!("· Databases      none detected");
    } else {
        for d in &env.databases {
            println!("✓ Database      {}", d);
        }
    }

    let required = ctx_env::required_env_vars(&root);
    let missing: Vec<_> = required
        .iter()
        .filter(|v| std::env::var(v).is_err())
        .cloned()
        .collect();
    if !missing.is_empty() {
        println!();
        println!("⚠ Missing environment variables:");
        for v in &missing {
            println!("  {}", v);
        }
    }
    println!();
    println!(
        "Status: {}",
        if missing.is_empty() {
            "Ready"
        } else {
            "Needs configuration"
        }
    );
    Ok(())
}

pub(crate) fn mark(ok: bool) -> &'static str {
    if ok {
        "✓"
    } else {
        "✗"
    }
}
pub(crate) fn print_opt(label: &str, v: &Option<String>) {
    match v {
        Some(val) => println!("✓ {:<14} {}", label, val),
        None => println!("✗ {:<14} not found", label),
    }
}

pub(crate) fn cmd_history() -> Result<()> {
    let (ctx, _) = open_ctx()?;
    let cps = ctx.load_checkpoints()?;
    if cps.is_empty() {
        println!("No checkpoints yet.");
        return Ok(());
    }
    for cp in &cps {
        println!(
            "{}  {}  {}  files:{}  {}",
            cp.id,
            cp.created_at,
            cp.branch.as_deref().unwrap_or("-"),
            cp.files_changed,
            cp.summary.as_deref().unwrap_or("")
        );
    }
    Ok(())
}

pub(crate) fn cmd_inspect(what: &str) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    let state = ctx.load_state()?;
    match what {
        "decisions" => {
            for d in &state.decisions {
                let mut tags = String::new();
                if let Some(s) = &d.superseded_by {
                    tags.push_str(&format!(" [superseded by {}]", s));
                }
                if let Some(c) = &d.conflicts_with {
                    tags.push_str(&format!(" [⚠ possible conflict with {}]", c));
                }
                println!(
                    "{}: {} — {}{}",
                    d.id,
                    d.decision,
                    d.reason.as_deref().unwrap_or(""),
                    tags
                );
            }
            if state.decisions.is_empty() {
                println!("No decisions recorded.");
            }
        }
        "tasks" => {
            for t in &state.tasks {
                println!("[{}] {}", t.status, t.title);
            }
            if state.tasks.is_empty() {
                println!("No tasks recorded.");
            }
        }
        "environment" => {
            println!("{}", serde_json::to_string_pretty(&state.environment)?);
        }
        "agents" => {
            println!("Detected agents: (adapters not yet implemented)");
        }
        other => anyhow::bail!(
            "unknown inspect target: {} (decisions|tasks|environment|agents)",
            other
        ),
    }
    Ok(())
}

pub(crate) fn cmd_checkpoint(summary: Option<String>) -> Result<()> {
    let (ctx, root) = open_ctx()?;
    let mut config = ctx.load_config()?;
    let git = ctx_git::info(&root);
    let state = ctx.load_state()?;

    let mut cps = ctx.load_checkpoints()?;
    let id = ctx_core::alloc_checkpoint_id(&mut config.checkpoint_counter, &cps);
    cps.push(Checkpoint {
        id: id.clone(),
        git_commit: git.last_commit.clone(),
        branch: git.branch.clone(),
        agent: None,
        task: state.objective.clone(),
        files_changed: git.uncommitted_count,
        files: git.dirty_files.clone(),
        status: state.status.clone(),
        created_at: chrono_now(),
        summary: summary.clone(),
    });
    ctx.save_config(&config)?;
    ctx.save_checkpoints(&cps)?;
    println!(
        "Checkpoint {} created{}",
        id,
        summary.map(|s| format!(": {}", s)).unwrap_or_default()
    );
    Ok(())
}

pub(crate) fn cmd_instructions(path: Option<String>) -> Result<()> {
    let (_, root) = open_ctx()?;
    let cwd = match path {
        Some(p) => std::path::Path::new(&p)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(&p)),
        None => project_root(),
    };
    let list = ctx_context::instructions_for(&root, &cwd);
    if list.is_empty() {
        println!("No instruction files found for {}", cwd.display());
        return Ok(());
    }
    println!("Applicable instructions (shallow → deep):");
    for p in &list {
        println!("  {}", p.strip_prefix(&root).unwrap_or(p).display());
    }
    Ok(())
}

pub(crate) fn cmd_agents() -> Result<()> {
    let root = project_root();
    println!("Detected agents / session sources:");
    for (name, found) in ctx_ingest::detected(&root) {
        println!("{} {}", if found { "✓" } else { "✗" }, name);
    }
    println!();
    println!("Generic fallback: ctx ingest --from <transcript.jsonl>");
    println!();
    println!("Note: VS Code / Copilot keeps no readable session transcript on");
    println!("disk, so there is no automatic reader for it. For Copilot the");
    println!("cooperative note (.ctx/handoffs/current.md, via AGENTS.md) plus");
    println!("git state is the designed channel — keep the note current and");
    println!("`ctx resume` will carry it to the next agent.");
    Ok(())
}

pub(crate) fn cmd_checkpoint_auto() -> Result<()> {
    let Ok((ctx, root)) = open_ctx() else {
        return Ok(());
    };
    let mut config = match ctx.load_config() {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };
    let git = ctx_git::info(&root);
    let now = chrono::Utc::now().to_rfc3339().to_string();
    let mut cps = ctx.load_checkpoints().unwrap_or_default();
    let id = ctx_core::alloc_checkpoint_id(&mut config.checkpoint_counter, &cps);
    cps.push(ctx_core::Checkpoint {
        id,
        git_commit: git.last_commit,
        branch: git.branch,
        agent: None,
        task: None,
        files_changed: git.uncommitted_count,
        files: git.dirty_files,
        status: "auto".into(),
        created_at: now,
        summary: Some("auto (post-commit)".into()),
    });
    let _ = ctx.save_config(&config);
    let _ = ctx.save_checkpoints(&cps);
    Ok(())
}

/// Opt-in commit helper: stages everything git sees except `.ctxignore`
/// matches, then commits. The post-commit hook auto-checkpoints.
/// Never silent — always prints what was skipped and the result.
pub(crate) fn cmd_commit(message: Option<String>) -> Result<()> {
    let (_ctx, root) = open_ctx()?;
    let Some(msg) = message else {
        anyhow::bail!("usage: ctx commit -m \"message\"");
    };
    let git = ctx_git::info(&root);
    if !git.is_repo {
        anyhow::bail!("not a git repository");
    }
    if git.dirty_files.is_empty() {
        println!("Nothing to commit — working tree is clean.");
        return Ok(());
    }
    let patterns = ctx_git::ctxignore_patterns(&root);
    let (skipped, included): (Vec<_>, Vec<_>) = git
        .dirty_files
        .iter()
        .partition(|f| ctx_git::ctxignored(&patterns, f));
    for f in &skipped {
        println!("Skipped by .ctxignore: {}", f);
    }
    if included.is_empty() {
        println!("Nothing to commit after .ctxignore filtering.");
        return Ok(());
    }
    let add = std::process::Command::new("git")
        .arg("add")
        .arg("--")
        .args(&included)
        .current_dir(&root)
        .status()?;
    if !add.success() {
        anyhow::bail!("git add failed");
    }
    let commit = std::process::Command::new("git")
        .arg("commit")
        .arg("-m")
        .arg(&msg)
        .current_dir(&root)
        .status()?;
    if !commit.success() {
        anyhow::bail!("git commit failed (see git output above)");
    }
    println!("Committed {} file(s): {}", included.len(), msg);
    Ok(())
}

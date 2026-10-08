use anyhow::Result;
use clap::{Parser, Subcommand};
use ctx_core::{Checkpoint, Config, EnvironmentState, ProjectContext};
use ctx_storage::CtxDir;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ctx", version, about = "CTX — Git for AI context")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init {
        #[arg(long)]
        yes: bool,
    },
    Status,
    Doctor,
    History,
    Handoff,
    Inspect {
        what: String,
    },
    Checkpoint {
        summary: Option<String>,
    },
    Resume {
        #[arg(long)]
        agent: Option<String>,
    },
    Recover,
    Diff,
    Watch,
    Task {
        title: String,
    },
    Decide {
        text: String,
        #[arg(long)]
        reason: Option<String>,
    },
    Complete {
        item: String,
    },
    Next {
        action: String,
    },
    Objective {
        text: String,
    },
    Instructions {
        path: Option<String>,
    },
    Ingest {
        from: Option<String>,
    },
    Agents,
    #[command(name = "checkpoint-auto")]
    CheckpointAuto,
    Hooks {
        #[command(subcommand)]
        action: HooksAction,
    },
}

#[derive(Subcommand)]
enum HooksAction {
    Install,
    Status,
}

fn project_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn open_ctx() -> Result<(CtxDir, PathBuf)> {
    let root = project_root();
    match CtxDir::discover(&root)? {
        Some(d) => {
            let project_root = d.root.parent().unwrap().to_path_buf();
            Ok((d, project_root))
        }
        None => anyhow::bail!("ctx not initialized in this directory (run `ctx init`)"),
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init { yes } => cmd_init(yes),
        Commands::Status => cmd_status(),
        Commands::Doctor => cmd_doctor(),
        Commands::History => cmd_history(),
        Commands::Handoff => cmd_handoff(),
        Commands::Inspect { what } => cmd_inspect(&what),
        Commands::Checkpoint { summary } => cmd_checkpoint(summary),
        Commands::Resume { agent } => cmd_resume(agent),
        Commands::Recover => cmd_recover(),
        Commands::Diff => cmd_diff(),
        Commands::Watch => cmd_watch(),
        Commands::Task { title } => cmd_task(title),
        Commands::Decide { text, reason } => cmd_decide(text, reason),
        Commands::Complete { item } => cmd_complete(item),
        Commands::Next { action } => cmd_next(action),
        Commands::Objective { text } => cmd_objective(text),
        Commands::Instructions { path } => cmd_instructions(path),
        Commands::Ingest { from } => cmd_ingest(from),
        Commands::Agents => cmd_agents(),
        Commands::CheckpointAuto => cmd_checkpoint_auto(),
        Commands::Hooks { action } => match action {
            HooksAction::Install => {
                let root = project_root();
                install_git_hook(&root);
                Ok(())
            }
            HooksAction::Status => {
                let root = project_root();
                let p = root.join(".git/hooks/post-commit");
                if p.exists() {
                    println!("post-commit hook: installed");
                } else {
                    println!("post-commit hook: not installed (run `ctx hooks install`)");
                }
                Ok(())
            }
        },
    }
}

fn detect_project_name(root: &std::path::Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".into())
}

fn cmd_init(yes: bool) -> Result<()> {
    let root = project_root();
    let ctx = CtxDir::at(&root);
    let already = ctx.exists();
    ctx.init()?;

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

    let config = if already && ctx.config_path().exists() {
        ctx.load_config().unwrap_or(Config {
            project_name: state.project.clone(),
            checkpoint_counter: 0,
            created_at: Some(chrono_now()),
        })
    } else {
        Config {
            project_name: state.project.clone(),
            checkpoint_counter: 0,
            created_at: Some(chrono_now()),
        }
    };
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

    inject_cooperative_note(&root);
    install_git_hook(&root);
    Ok(())
}

fn prompt_yes(question: &str) -> bool {
    use std::io::Write;
    print!("{}", question);
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).ok();
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
}

const NOTE_BLOCK_START: &str = "<!-- CTX:START -->";
const NOTE_BLOCK_END: &str = "<!-- CTX:END -->";

fn inject_cooperative_note(root: &std::path::Path) {
    let path = root.join("AGENTS.md");
    let block = format!(
        "{}\n## For AI coding agents (managed by CTX)\n\nMaintain `.ctx/handoffs/current.md` while working. Before ending a turn or when you are about to stop, update it with:\n\n- Goal\n- Completed\n- Current work\n- Next action\n- Blockers\n\nCTX reads this file to build the handoff for the next agent. Keep it short and factual.\n{}\n",
        NOTE_BLOCK_START, NOTE_BLOCK_END
    );
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
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
    if let Err(e) = std::fs::write(&path, updated) {
        eprintln!("warning: could not update AGENTS.md: {}", e);
    } else {
        println!("AGENTS.md updated with CTX note instructions.");
    }
}

fn install_git_hook(root: &std::path::Path) {
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

fn chrono_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn cmd_status() -> Result<()> {
    let (ctx, root) = open_ctx()?;
    let state = ctx.load_state()?;
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
    Ok(())
}

fn cmd_doctor() -> Result<()> {
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

fn mark(ok: bool) -> &'static str {
    if ok {
        "✓"
    } else {
        "✗"
    }
}
fn print_opt(label: &str, v: &Option<String>) {
    match v {
        Some(val) => println!("✓ {:<14} {}", label, val),
        None => println!("✗ {:<14} not found", label),
    }
}

fn cmd_history() -> Result<()> {
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

fn cmd_handoff() -> Result<()> {
    let (ctx, root) = open_ctx()?;
    let state = ctx.load_state()?;
    let git = ctx_git::info(&root);

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

fn cmd_inspect(what: &str) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    let state = ctx.load_state()?;
    match what {
        "decisions" => {
            for d in &state.decisions {
                println!(
                    "{}: {} — {}",
                    d.id,
                    d.decision,
                    d.reason.as_deref().unwrap_or("")
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

fn cmd_checkpoint(summary: Option<String>) -> Result<()> {
    let (ctx, root) = open_ctx()?;
    let mut config = ctx.load_config()?;
    let git = ctx_git::info(&root);
    let state = ctx.load_state()?;

    config.checkpoint_counter += 1;
    let id = ctx_core::next_checkpoint_id(config.checkpoint_counter);
    let mut cps = ctx.load_checkpoints()?;
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

fn cmd_resume(agent: Option<String>) -> Result<()> {
    let (ctx, root) = open_ctx()?;
    let state = ctx.load_state()?;
    let git = ctx_git::info(&root);
    let agent_name = agent.as_deref().unwrap_or("generic");
    let adapter = ctx_adapters::get(agent_name).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown agent '{}' (available: generic, opencode)",
            agent_name
        )
    })?;
    let mut rendered = adapter.render(&state, &git);
    // provenance: say where each part came from
    rendered.push_str("\n---\nSources: project/git state");
    if state.ingested.is_some() {
        rendered.push_str(" + ingested AI session");
    }
    if root.join(".ctx/handoffs/current.md").exists() {
        rendered.push_str(" + AI cooperative note");
    }
    rendered.push('\n');
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
        if !ing.errors.is_empty() {
            rendered.push_str("Errors:\n");
            for e in &ing.errors {
                rendered.push_str(&format!("- {}\n", e));
            }
        }
    }
    let cwd = project_root();
    let merged = ctx_context::merged_instructions(&root, &cwd);
    if !merged.is_empty() {
        rendered.push_str("\n## Project instructions\n\n");
        rendered.push_str(&merged);
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
    Ok(())
}

fn cmd_recover() -> Result<()> {
    let (ctx, root) = open_ctx()?;
    let state = ctx.load_state()?;
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

fn cmd_diff() -> Result<()> {
    let (ctx, root) = open_ctx()?;
    let git = ctx_git::info(&root);
    let cps = ctx.load_checkpoints()?;

    let last: Vec<String> = cps.last().map(|c| c.files.clone()).unwrap_or_default();
    let now: Vec<String> = git.dirty_files.clone();
    let added: Vec<_> = now.iter().filter(|f| !last.contains(f)).collect();
    let removed: Vec<_> = last.iter().filter(|f| !now.contains(f)).collect();

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
    Ok(())
}

fn cmd_watch() -> Result<()> {
    let (ctx, root) = open_ctx()?;
    ctx_observer::watch(root, ctx.root)
}

fn cmd_task(title: String) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    let mut state = ctx.load_state()?;
    state.tasks.push(ctx_core::Task {
        title,
        status: "pending".into(),
        notes: None,
    });
    ctx.save_state(&state)?;
    println!("Task added.");
    Ok(())
}

fn cmd_decide(text: String, reason: Option<String>) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    let mut state = ctx.load_state()?;
    let id = format!("CTX-{}", state.decisions.len() + 1);
    state.decisions.push(ctx_core::Decision {
        id,
        decision: text,
        reason,
        recorded_at: Some(chrono::Utc::now().to_rfc3339()),
    });
    ctx.save_state(&state)?;
    println!("Decision recorded.");
    Ok(())
}

fn cmd_complete(item: String) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    let mut state = ctx.load_state()?;
    state.completed.push(item);
    ctx.save_state(&state)?;
    println!("Marked complete.");
    Ok(())
}

fn cmd_next(action: String) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    let mut state = ctx.load_state()?;
    state.next_action = Some(action.clone());
    ctx.save_state(&state)?;
    println!("Next action set: {}", action);
    Ok(())
}

fn cmd_objective(text: String) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    let mut state = ctx.load_state()?;
    state.objective = Some(text.clone());
    state.status = "in_progress".into();
    ctx.save_state(&state)?;
    println!("Objective set: {}", text);
    Ok(())
}

fn cmd_instructions(path: Option<String>) -> Result<()> {
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

fn cmd_ingest(from: Option<String>) -> Result<()> {
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

    let compressed = ctx_ingest::compress(&session);
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
        errors: compressed.errors.clone(),
        files_touched: compressed.files_touched.clone(),
        commands: compressed.commands.clone(),
        source: compressed.source.clone(),
    });
    ctx.save_state(&state)?;

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

fn parse_generic_jsonl(
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

fn cmd_agents() -> Result<()> {
    let root = project_root();
    println!("Detected agents / session sources:");
    for (name, found) in ctx_ingest::detected(&root) {
        println!("{} {}", if found { "✓" } else { "✗" }, name);
    }
    println!();
    println!("Generic fallback: ctx ingest --from <transcript.jsonl>");
    Ok(())
}

fn cmd_checkpoint_auto() -> Result<()> {
    let Ok((ctx, root)) = open_ctx() else {
        return Ok(());
    };
    let mut config = match ctx.load_config() {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };
    let git = ctx_git::info(&root);
    let now = chrono::Utc::now().to_rfc3339().to_string();
    config.checkpoint_counter += 1;
    let id = ctx_core::next_checkpoint_id(config.checkpoint_counter);
    let mut cps = ctx.load_checkpoints().unwrap_or_default();
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

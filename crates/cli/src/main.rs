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
    Init,
    Status,
    Doctor,
    History,
    Handoff,
    Inspect { what: String },
    Checkpoint { summary: Option<String> },
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
        Commands::Init => cmd_init(),
        Commands::Status => cmd_status(),
        Commands::Doctor => cmd_doctor(),
        Commands::History => cmd_history(),
        Commands::Handoff => cmd_handoff(),
        Commands::Inspect { what } => cmd_inspect(&what),
        Commands::Checkpoint { summary } => cmd_checkpoint(summary),
    }
}

fn detect_project_name(root: &std::path::Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".into())
}

fn cmd_init() -> Result<()> {
    let root = project_root();
    let ctx = CtxDir::at(&root);
    ctx.init()?;

    let git = ctx_git::info(&root);
    let docs = ctx_context::discover(&root);
    let env = ctx_env::detect(&root);

    let mut state = ProjectContext {
        project: detect_project_name(&root),
        status: "initialized".into(),
        ..Default::default()
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

    let config = Config {
        project_name: state.project.clone(),
        checkpoint_counter: 0,
        created_at: Some(chrono_now()),
    };
    ctx.save_config(&config)?;
    ctx.save_state(&state)?;
    ctx.save_checkpoints(&[])?;

    println!("Initialized CTX in {}", ctx.root.display());
    println!("Project: {}", state.project);
    println!(
        "Git: {}",
        if git.is_repo {
            "detected"
        } else {
            "not detected"
        }
    );
    println!("Instruction files found: {}", docs.instructions.len());
    println!("Runtimes: {:?}", state.environment.runtimes);
    Ok(())
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

use anyhow::Result;
use clap::{Parser, Subcommand};
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
    Handoff {
        #[arg(long)]
        agent: Option<String>,
    },
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
    Task {
        title: String,
    },
    Decide {
        text: String,
        #[arg(long)]
        reason: Option<String>,
        /// Mark an existing decision as replaced by this one.
        #[arg(long)]
        supersedes: Option<String>,
    },
    Resolve {
        id: String,
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
    Monitor {
        #[arg(long, default_value = "30")]
        interval: u64,
        #[arg(long)]
        once: bool,
    },
    #[command(name = "checkpoint-auto")]
    CheckpointAuto,
    Commit {
        #[arg(short, long)]
        message: Option<String>,
    },
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

pub(crate) fn project_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub(crate) fn open_ctx() -> Result<(CtxDir, PathBuf)> {
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
        Commands::Handoff { agent } => cmd_handoff(agent),
        Commands::Inspect { what } => cmd_inspect(&what),
        Commands::Checkpoint { summary } => cmd_checkpoint(summary),
        Commands::Resume { agent } => cmd_resume(agent),
        Commands::Recover => cmd_recover(),
        Commands::Diff => cmd_diff(),
        Commands::Task { title } => cmd_task(title),
        Commands::Decide {
            text,
            reason,
            supersedes,
        } => cmd_decide(text, reason, supersedes),
        Commands::Resolve { id } => cmd_resolve(id),
        Commands::Complete { item } => cmd_complete(item),
        Commands::Next { action } => cmd_next(action),
        Commands::Objective { text } => cmd_objective(text),
        Commands::Instructions { path } => cmd_instructions(path),
        Commands::Ingest { from } => cmd_ingest(from),
        Commands::Agents => cmd_agents(),
        Commands::Monitor { interval, once } => cmd_monitor(interval, once),
        Commands::CheckpointAuto => cmd_checkpoint_auto(),
        Commands::Commit { message } => cmd_commit(message),
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

pub(crate) fn detect_project_name(root: &std::path::Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".into())
}

mod cmd_project;
mod cmd_session;
mod cmd_state;
mod note;

use cmd_project::*;
use cmd_session::*;
use cmd_state::*;

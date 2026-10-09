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
    /// Initialize CTX in this project (.ctx/, agent note, git hook)
    Init {
        /// Run git init without asking when this folder is not a repo yet
        #[arg(long)]
        yes: bool,
        /// Declare your agent(s), comma-separated: claude, gemini, codex,
        /// copilot, opencode. Writes the note into their instruction file
        /// and makes bare `ctx resume` default to it.
        #[arg(long)]
        agent: Option<String>,
    },
    /// Show project state, checkpoint age, note freshness, conflicts
    Status,
    /// Check runtimes, package managers, docker, env vars
    Doctor,
    /// List checkpoints (newest last)
    History,
    /// Print the current handoff, optionally flavored per agent
    Handoff {
        /// Flavor the output: generic, opencode, claude, gemini, codex, copilot
        #[arg(long)]
        agent: Option<String>,
    },
    /// Show stored state: decisions | tasks | environment | agents
    Inspect {
        /// What to show: decisions | tasks | environment | agents
        what: String,
    },
    /// Record a manual checkpoint of current state
    Checkpoint {
        /// One-line note stored with the checkpoint
        summary: Option<String>,
    },
    /// Build the handoff file for the next agent to read
    Resume {
        /// Flavor the output: generic, opencode, claude, gemini, codex, copilot
        #[arg(long)]
        agent: Option<String>,
    },
    /// Reconstruct last known state after a crash or lost session
    Recover,
    /// Show working-tree changes since the last checkpoint
    Diff,
    /// Add a task to the current work list
    Task {
        /// Short task title
        title: String,
    },
    /// Record a lasting decision (flags possible contradictions)
    Decide {
        /// The decision in one sentence
        text: String,
        /// Why this choice was made
        #[arg(long)]
        reason: Option<String>,
        /// Mark an existing decision as replaced by this one.
        #[arg(long)]
        supersedes: Option<String>,
    },
    /// Clear a decision's conflict flag after human review
    Resolve {
        /// Decision id to clear, e.g. CTX-2
        id: String,
    },
    /// Mark a piece of work as completed
    Complete {
        /// What got finished
        item: String,
    },
    /// Set the single next action
    Next {
        /// The single next action
        action: String,
    },
    /// Set the current objective
    Objective {
        /// Current goal in one or two sentences
        text: String,
    },
    /// List instruction files applying to a directory
    Instructions {
        /// Directory to resolve from (default: current directory)
        path: Option<String>,
    },
    /// Import the newest AI session into .ctx state
    Ingest { from: Option<String> },
    /// Show which AI session sources this machine can read
    Agents,
    /// Keep .ctx fresh from live AI sessions (Ctrl-C to stop)
    Monitor {
        /// Seconds between session polls
        #[arg(long, default_value = "30")]
        interval: u64,
        /// Poll once and exit instead of looping
        #[arg(long)]
        once: bool,
    },
    #[command(name = "checkpoint-auto", hide = true)]
    CheckpointAuto,
    /// Commit work in progress, skipping .ctxignore matches
    Commit {
        /// Commit message (required)
        #[arg(short, long)]
        message: Option<String>,
    },
    /// Manage the git post-commit auto-checkpoint hook
    Hooks {
        #[command(subcommand)]
        action: HooksAction,
    },
}

#[derive(Subcommand)]
enum HooksAction {
    /// Install the post-commit hook into .git/hooks
    Install,
    /// Show whether the post-commit hook is installed
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
        Commands::Init { yes, agent } => cmd_init(yes, agent),
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

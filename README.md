# CTX

Git for AI context. A local-first CLI that preserves project state, AI context,
development decisions, and environment requirements so you can move between AI
agents, sessions, and machines without losing your place.

## The loop

```bash
ctx init                  # once per project: .ctx/, git hook, agent note
# ... work with any AI agent; every git commit auto-checkpoints ...
ctx monitor --once        # pull the latest AI session into .ctx (or run `ctx monitor` live)
ctx resume --agent gemini # hand off to the next agent
```

## Commands

```
ctx init [--yes]          Initialize CTX (offers git init, installs hook + agent note)
ctx status                Show current project state
ctx doctor                Check environment (runtimes, docker, env vars)
ctx history               List checkpoints
ctx checkpoint [summary]  Create a manual checkpoint
ctx commit -m "msg"      Commit, skipping .ctxignore matches (hook auto-checkpoints)
ctx handoff [--agent X]   Render a markdown handoff (claude|gemini|codex|copilot|opencode|generic)
ctx inspect <what>        decisions | tasks | environment | agents
ctx resume [--agent X]    Prepare agent context
ctx recover               Reconstruct last known state
ctx diff                  Context changes since last checkpoint
ctx monitor [--interval N] [--once]  Keep .ctx fresh from live AI sessions
ctx ingest [--from file]  Import the newest AI session for this project
ctx agents                Show detected session sources on this machine
ctx hooks install|status  Manage the git post-commit hook
ctx objective <text>      Set current objective
ctx task <title>          Add a task
ctx decide <text>         Record a decision (--reason ...)
ctx complete <item>       Mark work complete
ctx next <action>         Set next action
ctx instructions [path]   Show merged instruction hierarchy
```

## How ctx knows where the AI stopped

Three layers, best available wins (and `ctx resume` says which it used):

1. **Cooperative note** — `ctx init` teaches the agent (via AGENTS.md,
   CLAUDE.md, GEMINI.md) to maintain `.ctx/handoffs/current.md` after
   every milestone. Works with any agent that reads instruction files.
2. **Session ingestion** — `ctx ingest` / `ctx monitor` read Claude Code
   transcripts and the OpenCode session store (commands run, files
   touched, failed exits, last messages). Summaries only — raw
   transcripts are never persisted.
3. **Reconstruction** — git state, checkpoints, `ctx doctor`. Always works.

## A note on Copilot / VS Code

VS Code keeps no readable Copilot transcript on disk, so there is no
automatic session reader for it. For Copilot projects the cooperative
note plus git state is the designed channel: keep
`.ctx/handoffs/current.md` current (the agent does this when asked, per
AGENTS.md) and `ctx resume` will carry it to the next agent.

## Build

```
cargo build
cargo test
cargo install --path crates/cli
```

## License

MIT

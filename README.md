# CTX — Git for AI context

[![release](https://img.shields.io/github/v/release/Idrisanu/ctx)](https://github.com/Idrisanu/ctx/releases)
[![license](https://img.shields.io/github/license/Idrisanu/ctx)](LICENSE)
[![build](https://img.shields.io/github/actions/workflow/status/Idrisanu/ctx/release.yml?label=release)](https://github.com/Idrisanu/ctx/actions)

Your AI shouldn't forget just because you changed the tool.

CTX is an open-source, local-first CLI that preserves project state, AI
context, development decisions, and environment requirements — so you can
move between AI agents, sessions, and machines without losing where you
left off. Hit a token limit in Codex at 2am? Open the same folder in
another tool, run `ctx resume`, and keep going.

```bash
# Agent 1 (e.g. Codex) does half the work, hits its limit…
ctx init --agent codex
# ...work happens, every git commit auto-checkpoints...

# Agent 2 (e.g. Gemini) picks up exactly where it stopped:
ctx resume
# → goal, completed work, changed files, next step, project rules
```

No account. No cloud. No telemetry. Everything lives in `.ctx/` inside
your project and travels with your repo.

## How it works

CTX keeps a **canonical project state** outside any single AI session and
rebuilds it from three layers, best available wins:

| Layer | Source | What it captures |
|---|---|---|
| **A — Cooperative note** | The agent maintains `.ctx/handoffs/current.md` (instructed via `AGENTS.md` / `GEMINI.md` / `CLAUDE.md`) | Goal, completed, current work, last commands, next action, blockers |
| **B — Session ingestion** | `ctx ingest` / `ctx monitor` read Claude Code transcripts, the OpenCode session store, and Gemini CLI chats | Prompts, replies, commands run, files touched, errors, token usage |
| **C — Reconstruction** | Git state, checkpoints, `ctx doctor` | Changed files, branch, commits, environment. Always works. |

Every `ctx resume` labels which layers produced it, and never claims data
it doesn't have. If the note is missing and no transcript exists, ctx says
so instead of inventing context.

## Install

No Rust needed — download a prebuilt binary (Linux, macOS Intel/ARM, Windows):

**Linux / macOS:**
```bash
curl -fsSL https://raw.githubusercontent.com/Idrisanu/ctx/main/install.sh | sh
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/Idrisanu/ctx/main/install.ps1 | iex
```

This installs the latest `v*` release to `~/.local/bin` (or
`%USERPROFILE%\.local\bin` on Windows). Pin a version with
`CTX_VERSION=v0.1.0` (sh) / `$env:CTX_VERSION="v0.1.0"` (PowerShell).

### Linux — step by step

1. Open a terminal. No prerequisites (curl is preinstalled on virtually
   all distros; the `ctx` binary itself needs nothing).
2. Run:
   ```bash
   curl -fsSL https://raw.githubusercontent.com/Idrisanu/ctx/main/install.sh | sh
   ```
3. If the installer says `~/.local/bin is not on your PATH`, add it:
   ```bash
   echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc   # or ~/.zshrc
   source ~/.bashrc   # or open a new terminal
   ```
4. Verify: `ctx --help` lists all commands.
5. First run in a project: `cd your-project && ctx init`.

### macOS — step by step

1. Open Terminal. No prerequisites.
2. Run:
   ```bash
   curl -fsSL https://raw.githubusercontent.com/Idrisanu/ctx/main/install.sh | sh
   ```
   (Apple Silicon downloads the `aarch64` build, Intel the `x86_64` one.)
3. If `~/.local/bin` isn't on your PATH:
   ```bash
   echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
   source ~/.zshrc   # or open a new terminal
   ```
4. First launch: macOS Gatekeeper blocks unsigned binaries. Either
   right-click the `ctx` binary → **Open** once, or run:
   ```bash
   xattr -d com.apple.quarantine $(which ctx)
   ```
5. Verify: `ctx --help`. First run in a project: `ctx init`.

### Windows — step by step

1. Open PowerShell (no admin needed, no prerequisites).
2. Run:
   ```powershell
   irm https://raw.githubusercontent.com/Idrisanu/ctx/main/install.ps1 | iex
   ```
3. If `%USERPROFILE%\.local\bin` isn't on your PATH: open *System
   Properties → Environment Variables*, append it to your user `Path`,
   then open a new terminal.
4. First run: SmartScreen may warn since the binary is unsigned —
   choose **More info → Run anyway**.
5. Verify: `ctx --help`. First run in a project: `cd your-project; ctx init`.

> **macOS note:** the binary isn't Apple-signed, so Gatekeeper blocks the
> first run. Right-click `ctx` → Open once, or run:
> `xattr -d com.apple.quarantine $(which ctx)`.
> **Windows note:** SmartScreen may warn on first run — choose "Run anyway".

**From source** (requires Rust 1.85+; the binary itself has no runtime deps):
```bash
git clone https://github.com/Idrisanu/ctx
cd ctx
cargo install --path crates/cli
ctx --help
```

After code changes, re-run `cargo install --path crates/cli` to refresh
your local binary.

## Quick start

```bash
cd your-project
ctx init --agent gemini   # one time: .ctx/, git hook, agent instruction
git add -A && git commit -m "init"

# ...work with your AI agent...

ctx status                # where things stand
ctx monitor --once        # pull the latest AI session into .ctx
ctx resume                # handoff file for the next agent
```

## Commands

### Setup

```
ctx init [--yes] [--agent <name,...>]
```
One-time setup per project: creates `.ctx/`, installs a git post-commit
hook (every commit becomes a checkpoint automatically), writes the
cooperative-note instruction into your agent's file, and detects readable
AI sessions. Offers to run `git init` if needed. `--agent gemini` writes
to `GEMINI.md` (`claude` → `CLAUDE.md`, `copilot` → copilot instructions,
`codex`/`opencode` → `AGENTS.md`) and makes bare `ctx resume` default to
that agent. Re-running refreshes everything without losing state.

```
ctx agents
```
Show which AI session sources this machine can read (claude-code,
opencode, gemini). Notes honestly when a tool keeps no transcript
(Copilot/VS Code) and what to do instead.

```
ctx doctor
```
Environment check: git, Node/Python/Rust/Go, pnpm/npm, Docker, databases,
and required-but-missing env vars (**names only — values never leave
your machine**). Run this first on a new machine.

```
ctx hooks install|status
```
Manage the git post-commit auto-checkpoint hook (installed by `init`).

### Daily use

```
ctx status
```
Project, objective, branch, changed-file count, checkpoint age, AI-note
freshness (warns when stale), last ingested session, and any decision
conflicts. Warns about uncommitted work before agent switches.

```
ctx resume [--agent <name>]
```
Build the handoff file (`.ctx/handoffs/<agent>.md`) for the next agent:
goal, completed, current work, changed files, uncommitted frontier,
recent commands, errors, decisions, project instructions, project-doc
pointers, and provenance. Flavors: `generic`, `opencode`, `claude`,
`gemini`, `codex`, `copilot`. Defaults to your `init --agent`, else
generic. Warns when the tree is dirty.

```
ctx handoff [--agent <name>]
```
Print the current handoff to the terminal instead of writing a file.

```
ctx monitor [--interval N] [--once]
```
Poll live AI sessions and fold anything new into `.ctx/state.json`.
Run it in the background while an agent works; `--once` for a single
pass (useful in scripts). On token death, state is current to the last
minutes, not the last commit.

```
ctx ingest [--from <transcript.jsonl>]
```
One-shot import of the newest AI session for this project. `--from`
points at any JSONL transcript (the generic reader for tools without a
built-in one).

```
ctx switch <agent>
```
One ritual step for changing tools: checkpoints where the outgoing
agent stopped, archives its note (`.ctx/handoffs/<agent>-<date>.md`),
points future bare `ctx resume` at the new agent, and renders the
incoming handoff.

```
ctx verify
```
Did the new agent follow the handoff? Compares stated intent
(objective, current work, next action) against observed reality
(working-tree changes, recent commits, note freshness) and flags drift.
Reports evidence, never pretends semantic understanding.

```
ctx recover
```
Reconstruct last known state after a crash or lost session: checkpoint,
task, changed files, next action.

```
ctx diff
```
Working-tree changes since the last checkpoint (added vs resolved).

```
ctx commit -m "message" [--yes]
```
Commit work in progress while skipping `.ctxignore` matches (`.env`,
`*.pem`, `secrets/`). Stops for confirmation when staging is large
(100+ files) or includes dependency dirs (`node_modules/`, …) — unless
`--yes`. Surfaces git's real error output. Explicit and loud —
ctx never auto-commits on its own.

```
ctx completion <shell>
```
Print shell completions (`bash|zsh|fish|powershell|elvish`) to stdout
for piping into your shell's completion dir.

### State (optional, never required for correctness)

```
ctx objective "<goal>"      Set the current objective
ctx task "<title>"          Add a task
ctx complete "<item>"       Mark work complete
ctx next "<action>"         Set the single next action
ctx decide "<text>" [--reason "..."] [--supersedes CTX-N]
                            Record a lasting decision. Warns on possible
                            contradictions with recorded decisions; history
                            is kept, never deleted.
ctx resolve <CTX-N>         Clear a decision's conflict flag after review
ctx checkpoint ["summary"]  Record a manual checkpoint
ctx history                 List checkpoints (newest last)
ctx instructions [path]     List instruction files applying to a directory
ctx inspect <what>          decisions | tasks | environment | agents
```

Manual commands exist for control. If your agent keeps its note and you
commit normally, you may never need them.

## The token-limit scenario

1. `ctx init --agent gemini` in the project (once).
2. Work. The agent maintains `.ctx/handoffs/current.md`; commits
   auto-checkpoint; `ctx monitor` (optional) tracks the live session.
3. Token limit hits mid-milestone.
4. `ctx resume` (or `--agent <next-tool>`) → paste the handoff file into
   the next agent as its first message.
5. It continues from goal + completed + uncommitted frontier + next step.

## Project documents

`ctx resume` embeds your instruction files (`AGENTS.md` hierarchy, root
→ deepest) in full, and points the next agent at your planning docs
(`PRD.md`, `MVP.md`, `milestone-*.md`, `ROADMAP.md`, READMEs) by path —
so it reads the authoritative text instead of a stale copy.

## Agent support

| Agent | Cooperative note | Transcript ingestion | Handoff flavor |
|---|---|---|---|
| Claude Code | `CLAUDE.md` | `~/.claude/projects/*.jsonl` | `claude` |
| OpenCode | `AGENTS.md` | session SQLite store | `opencode` |
| Gemini CLI | `GEMINI.md` | `~/.gemini/tmp/*/chats/*` | `gemini` |
| Codex | `AGENTS.md` | planned | `codex` |
| Copilot / VS Code | copilot instructions | none on disk — note + git is the design | `copilot` |
| Anything else | `AGENTS.md` | `ctx ingest --from file.jsonl` | `generic` |

New agents slot in as readers + flavors; the core never hard-codes a vendor.

## Privacy

- Local-first, offline-capable. No account, no telemetry, no cloud.
- Ingestion stores summaries/excerpts only — raw transcripts are never
  persisted into `.ctx/`.
- Secrets are never copied: `.ctxignore` (`.env`, `*.pem`, `secrets/…`)
  is honored by `ctx commit`; `ctx doctor` reports env var names, never values.

## Build & develop

```bash
cargo build
cargo test          # 13+ unit tests, fixtures included
cargo clippy --all  # must be zero warnings
cargo fmt           # run before every commit
```

Workspace crates: `cli`, `core` (pure domain logic), `storage` (`.ctx`
I/O), `git`, `env`, `context` (instruction discovery), `adapters`
(per-agent rendering), `ingest` (session readers).

## Roadmap

- `ctx switch --agent X` (checkpoint + archive + render in one step)
- `ctx verify` (did the new agent follow the handoff?)
- `ctx restore` semantics, `ctx why`
- Prebuilt binaries + one-line install for Linux/macOS/Windows
- Codex transcript reader

## License

MIT — see [LICENSE](LICENSE).

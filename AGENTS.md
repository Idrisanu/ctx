# CTX repository rules

Rust workspace. One binary (`ctx`), library crates per concern.

## Build, test, lint

```
cargo build
cargo test
cargo clippy --all        # must be clean (zero warnings)
cargo fmt                 # run before every commit
cargo install --path crates/cli   # refresh the local `ctx` binary
```

## Conventions

- No new top-level dependencies without a reason in the commit message.
- `ctx-core` holds pure domain logic with unit tests; no I/O there.
- I/O lives in `ctx-storage` (`.ctx/` files) and the CLI command modules.
- Never persist secrets: env var names only, transcript summaries only.
- Never claim data ctx doesn't have — print `Unknown`, not estimates.
- Keep clippy at zero warnings; fix lints instead of allowing them.

## Commits

Short imperative summaries, e.g. `M4.1: conflict detection (...)`.
`cargo fmt` + `cargo test` + `cargo clippy --all` must pass first.

<!-- CTX:START -->
## For AI coding agents (managed by CTX)

Maintain `.ctx/handoffs/current.md` while working. Update it after every milestone, every batch of tool calls, and before ending a turn — not only at the end. Include:

- Goal
- Completed
- Current work
- Last command run (and its result)
- Last error, if any
- Next action
- Blockers

CTX reads this file to build the handoff when the session ends (token limit, crash, or tool switch). Keep it short and factual.

When you establish a lasting pattern or make an architectural choice, also record it with `ctx decide "..." --reason "..."` so the next agent cannot silently contradict it.
<!-- CTX:END -->

# Contributing to CTX

Thanks for stopping by. CTX is early and small on purpose — contributions
that keep it that way are welcome.

## Setup

```bash
git clone https://github.com/Idrisanu/ctx
cd ctx
cargo build
```

Requires Rust 1.85+.

## Before every commit

```bash
cargo fmt
cargo test
cargo clippy --all   # must be zero warnings — fix lints, don't allow them
```

## Conventions

- **No new top-level dependencies** without a reason in the commit message.
- `ctx-core` holds pure domain logic with unit tests; no I/O there.
- I/O lives in `ctx-storage` (`.ctx/` files) and the CLI command modules.
- **Never persist secrets:** env var names only, transcript summaries only.
- **Never claim data ctx doesn't have** — print `Unknown`, not estimates.
- Commits: short imperative summaries, e.g. `M7: ctx switch (...)`.

## Product guardrails (for feature PRs)

- Core stays offline-first, local-first, no accounts, no telemetry.
- New agents slot in as readers + output flavors; the core never
  hard-codes a vendor.
- If a feature needs a cloud service to work, it doesn't belong in core.

## Reporting bugs

Use the bug template. Always include: `ctx --version`, OS, which AI
agent you used, `ctx status` output, and (if relevant) the handoff
excerpt that went wrong. Never paste secrets or API keys.

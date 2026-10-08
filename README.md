# CTX

Git for AI context. A local-first CLI that preserves project state, AI context,
development decisions, and environment requirements so you can move between AI
agents, sessions, and machines without losing your place.

## Status

Milestone 0 — foundation. See `/home/loma/.opencode/plan/ctx-plan.md`.

## Commands

```
ctx init        Initialize CTX in the current project
ctx status      Show current project state
ctx doctor      Check environment (runtimes, docker, env vars)
ctx history     List checkpoints
ctx checkpoint  Create a manual checkpoint
ctx handoff     Render a markdown handoff
ctx inspect <what>  decisions | tasks | environment | agents
```

## Build

```
cargo build
cargo test
```

## License

MIT

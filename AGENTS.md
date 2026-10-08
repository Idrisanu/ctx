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
<!-- CTX:END -->

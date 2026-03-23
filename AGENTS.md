# Agent Rules

Global working rules for this repository. Keep this file small and stable.

## Operating Principles

- Gather context before coding. Do not guess.
- Read existing tests to infer expected behavior.
- Prefer bash commands over PowerShell/cmd.
- Use Unix-style paths (`/`) in commands and docs.
- Read test logs in `target/test-logs/` for debugging instead of relying on truncated test stdout.
- Keep scope tight: do not add extra features or broad refactors unless requested.

## Discovery Protocol (Before Editing)

Use live sources first:

1. Documentation: use doc-viewer MCP tools to locate relevant module docs.
2. Known issues/plans: use ticket-mcp tools before duplicating work.
3. Test failures: use log-viewer MCP tools (`get_log`, `search_all_logs`, `query_logs`).
4. Graph/workspace behavior: use context-mcp tools for context-engine operations.

Use static references as support:

1. `CHEAT_SHEET.md` for type-level gotchas and common patterns.
2. crate `README.md` and `HIGH_LEVEL_GUIDE.md` for design context.
3. existing tests for usage examples and assertions.

## Task Routing

- Simple fix (1-2 files): gather context, implement, test.
- Bug fix: follow `.github/prompts/debug-test.prompt.md` when available.
- Feature or refactor (>5 files, >100 LOC, or unclear scope): use `.github/prompts/tickets.prompt.md` and plan first.
- Unfamiliar module or unclear behavior: follow `.github/prompts/research.prompt.md` when available.
- Swarm execution: use `.github/prompts/swarm-worker.prompt.md`.

## Quality Gates

- Tests relevant to your change must pass before completion.
- For tracing-based tests, use:

```rust
let _tracing = init_test_tracing!(&graph);
```

- If public behavior or docs changed, run doc validation workflows.
- Follow `.github/hooks/` reminders when they fire.
- Scratch notes belong in temporary files only; do not commit ephemeral notes.

## Escalation Rules

- If blocked by ambiguity after focused research (10-15 minutes), ask the user.
- If evidence conflicts or architecture tradeoffs are required, ask before committing to a direction.
- If unexpected workspace changes appear that you did not make, stop and ask how to proceed.

## Fallback Mode (When MCP Is Unavailable)

- Docs fallback: search/read local docs directly.
- Ticket fallback: use `ticket` CLI.
- Logs fallback: inspect files under `target/test-logs/` directly.
- Context fallback: use `tools/context-cli/` commands.

## Canonical Sources

- API patterns and gotchas: `CHEAT_SHEET.md`
- Ticket workflow details: `.github/prompts/tickets.prompt.md`
- Swarm workflow details: `.github/prompts/swarm-worker.prompt.md`
- Path-specific rules: `.github/instructions/*.instructions.md`
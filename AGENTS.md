# Agent Rules

Global working rules for this repository. Keep this file small and stable.

## Operating Principles

The canonical shared operating principles are owned at the context-engine root and mirrored through memory-viewers. Add memory-api-specific rules here only when the shared owner is insufficient.

## Discovery Protocol (Before Editing)

Use live sources first:

The canonical discovery protocol is owned at the context-engine root and mirrored through memory-viewers. Add memory-api-specific discovery steps here only when the shared protocol is insufficient.

Use static references as support:

1. crate `README.md` and `HIGH_LEVEL_GUIDE.md` for type-level gotchas, common patterns, and design context.
2. existing tests for usage examples and assertions.

## Task Routing

Memory-api follows the root task-routing rules by default. Add local routing notes here only when API crates, storage/index work, or CLI/MCP/HTTP adapters need a narrower execution path than the shared workflow already provides.

## Quality Gates

Memory-api inherits the shared quality gates from the context-engine root. Reserve this local section for API-surface checks such as store invariants, transport envelopes, or schema evidence that the shared viewer-heavy rules do not already cover.

```rust
let _tracing = init_test_tracing!(&graph);
```

Add memory-api-specific trailing quality-gate reminders here only when the shared quality-gate owner at the context-engine root is insufficient.

## Feedback Workflow

The canonical feedback workflow is owned at the context-engine root and mirrored through memory-viewers. Add memory-api-specific feedback handling here only when the shared owner is insufficient.

## Escalation Rules

The canonical escalation rules are owned at the context-engine root and mirrored through memory-viewers. Add memory-api-specific escalation policy here only when the shared owner is insufficient.

## Fallback Mode (When MCP Is Unavailable)

- Docs fallback: search/read local docs directly.
- Ticket fallback: use `ticket` CLI.
- Logs fallback: inspect files under `target/test-logs/` directly.
- Context fallback: use `tools/context-cli/` commands.

## Canonical Sources

- API patterns and gotchas: crate `README.md`, `HIGH_LEVEL_GUIDE.md`, and existing tests
- Ticket workflow details: `.agents/prompts/tickets.prompt.md`
- Swarm workflow details: `.agents/prompts/swarm-worker.prompt.md`
- Path-specific rules: `.agents/instructions/*.instructions.md`

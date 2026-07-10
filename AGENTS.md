<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=42ffd955-ebdf-44b4-9092-bcf5c7b07a29 slug=shared/agent-rules/l1 -->
# Agent Rules

Global working rules for this repository. Keep this file small and stable.

<!-- rule-api:entry id=2180fedc-fb0f-4900-9411-e8a2c9534b2d slug=shared/agent-rules/operating-principles/l5 -->
## Operating Principles

The canonical shared operating principles are owned at the context-engine root and mirrored through memory-viewers. Add memory-api-specific rules here only when the shared owner is insufficient.

<!-- rule-api:entry id=dc2138dd-5d82-4508-929d-05a1ddf4c58c slug=shared/agent-rules/discovery-protocol-before-editing/l17 -->
## Discovery Protocol (Before Editing)

Use live sources first:

<!-- rule-api:entry id=7748871f-4c8b-4529-bb18-ca39b9bbad60 slug=shared/agent-rules/discovery-protocol-before-editing/l21 -->
The canonical discovery protocol is owned at the context-engine root and mirrored through memory-viewers. Add memory-api-specific discovery steps here only when the shared protocol is insufficient.

<!-- rule-api:entry id=c5496169-4b66-441d-b6b0-3ac35f707ddf slug=shared/agent-rules/discovery-protocol-before-editing/l31 -->
Use static references as support:

<!-- rule-api:entry id=c8dfa4df-fd57-44e0-97f6-3368aa039ee0 slug=shared/agent-rules/discovery-protocol-before-editing/l33 -->
1. `CHEAT_SHEET.md` for type-level gotchas and common patterns.
2. crate `README.md` and `HIGH_LEVEL_GUIDE.md` for design context.
3. existing tests for usage examples and assertions.

<!-- rule-api:entry id=facb3afd-0074-455b-8065-e54e1309399c slug=shared/agent-rules/task-routing/l37 -->
## Task Routing

Memory-api follows the root task-routing rules by default. Add local routing notes here only when API crates, storage/index work, or CLI/MCP/HTTP adapters need a narrower execution path than the shared workflow already provides.

<!-- rule-api:entry id=b6d18ba3-b1e6-471d-93c9-ff3d99e412b0 slug=shared/agent-rules/quality-gates/l46 -->
## Quality Gates

Memory-api inherits the shared quality gates from the context-engine root. Reserve this local section for API-surface checks such as store invariants, transport envelopes, or schema evidence that the shared viewer-heavy rules do not already cover.

<!-- rule-api:entry id=34054485-a4ff-47bf-b331-df68701cb967 slug=shared/agent-rules/quality-gates/l61 -->
```rust
let _tracing = init_test_tracing!(&graph);
```

<!-- rule-api:entry id=2d4b9e94-86e5-4abc-9d46-ad74c183b9f0 slug=shared/agent-rules/quality-gates/l65 -->
Add memory-api-specific trailing quality-gate reminders here only when the shared quality-gate owner at the context-engine root is insufficient.

<!-- rule-api:entry id=48ac3bab-a50d-40a7-9c8e-a452ae4c7f87 slug=shared/agent-rules/feedback-workflow/l70 -->
## Feedback Workflow

The canonical feedback workflow is owned at the context-engine root and mirrored through memory-viewers. Add memory-api-specific feedback handling here only when the shared owner is insufficient.

<!-- rule-api:entry id=4f6e76bb-90d4-4d90-9e06-ca03d134ae67 slug=shared/agent-rules/escalation-rules/l80 -->
## Escalation Rules

The canonical escalation rules are owned at the context-engine root and mirrored through memory-viewers. Add memory-api-specific escalation policy here only when the shared owner is insufficient.

<!-- rule-api:entry id=f7a6ba7b-81ef-40bb-ae98-49658dc3e337 slug=shared/agent-rules/fallback-mode-when-mcp-is-unavailable/l86 -->
## Fallback Mode (When MCP Is Unavailable)

- Docs fallback: search/read local docs directly.
- Ticket fallback: use `ticket` CLI.
- Logs fallback: inspect files under `target/test-logs/` directly.
- Context fallback: use `tools/context-cli/` commands.

<!-- rule-api:entry id=a3e94032-b6e0-44cc-857b-ca12b9647736 slug=shared/agent-rules/canonical-sources/l93 -->
## Canonical Sources

- API patterns and gotchas: `CHEAT_SHEET.md`
- Ticket workflow details: `.agents/prompts/tickets.prompt.md`
- Swarm workflow details: `.agents/prompts/swarm-worker.prompt.md`
- Path-specific rules: `.agents/instructions/*.instructions.md`

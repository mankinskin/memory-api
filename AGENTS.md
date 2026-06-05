<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=42ffd955-ebdf-44b4-9092-bcf5c7b07a29 slug=shared/agent-rules/l1 -->
# Agent Rules

Global working rules for this repository. Keep this file small and stable.

<!-- rule-api:entry id=2180fedc-fb0f-4900-9411-e8a2c9534b2d slug=shared/agent-rules/operating-principles/l5 -->
## Operating Principles

- Gather context before coding. Do not guess.
- Read existing tests to infer expected behavior.
- For implementation work, create or update the relevant ticket(s) before editing code.
- For new or changed requirements and goals, create or update the relevant spec before implementation proceeds.
- Keep the ticket, spec, validation, and documentation trail current so review and status summaries stay accurate.
- Prefer bash commands over PowerShell/cmd.
- Use Unix-style paths (`/`) in commands and docs.
- Read test logs in `target/test-logs/` for debugging instead of relying on truncated test stdout.
- Keep scope tight: do not add extra features or broad refactors unless requested.

<!-- rule-api:entry id=dc2138dd-5d82-4508-929d-05a1ddf4c58c slug=shared/agent-rules/discovery-protocol-before-editing/l17 -->
## Discovery Protocol (Before Editing)

Use live sources first:

<!-- rule-api:entry id=7748871f-4c8b-4529-bb18-ca39b9bbad60 slug=shared/agent-rules/discovery-protocol-before-editing/l21 -->
1. Documentation: use doc-viewer MCP tools to locate relevant module docs.
2. Known issues/plans: use ticket-mcp tools before duplicating work.
3. Board state: check active WIP, stale entries, and file ownership before touching
   implementation files — `mcp_ticket-mcp_board_show` with `{"workspace": "default"}` or:
   ```bash
  ./target/debug/ticket.exe board show --toon
   ```
4. Test failures: use log-viewer MCP tools (`get_log`, `search_all_logs`, `query_logs`).
5. Graph/workspace behavior: use context-mcp tools for context-engine operations.

<!-- rule-api:entry id=c5496169-4b66-441d-b6b0-3ac35f707ddf slug=shared/agent-rules/discovery-protocol-before-editing/l31 -->
Use static references as support:

<!-- rule-api:entry id=c8dfa4df-fd57-44e0-97f6-3368aa039ee0 slug=shared/agent-rules/discovery-protocol-before-editing/l33 -->
1. `CHEAT_SHEET.md` for type-level gotchas and common patterns.
2. crate `README.md` and `HIGH_LEVEL_GUIDE.md` for design context.
3. existing tests for usage examples and assertions.

<!-- rule-api:entry id=facb3afd-0074-455b-8065-e54e1309399c slug=shared/agent-rules/task-routing/l37 -->
## Task Routing

- Any requested implementation or behavior change: create or update the tracking ticket(s) first, then create or update the relevant spec before editing files.
- Simple fix (1-2 files): after the ticket/spec setup when requirements or behavior change, gather context, implement, validate, update docs, verify spec links, and move the ticket to `in-review`.
- Bug fix: after the ticket/spec setup, follow `.agents/prompts/debug-test.prompt.md` when available.
- Feature or refactor (>5 files, >100 LOC, or unclear scope): use `.agents/prompts/tickets.prompt.md` to establish the ticket set, then `.agents/prompts/spec.prompt.md` to update the spec before implementation.
- Unfamiliar module or unclear behavior: follow `.agents/prompts/research.prompt.md` when available before locking the spec or implementation plan.
- Swarm execution: use `.agents/prompts/swarm-worker.prompt.md`.

<!-- rule-api:entry id=b6d18ba3-b1e6-471d-93c9-ff3d99e412b0 slug=shared/agent-rules/quality-gates/l46 -->
## Quality Gates

- Relevant validation must pass before completion. If a required check repeatedly fails, stop expanding scope and record the failing command, log or manual result, and blocker clearly in the ticket/spec status summary.
- Before a ticket moves to `in-review`, ensure the relevant spec is updated for the changed requirements or goals and links the related tickets, updated docs, and test or validation results.
- **Browser verification is mandatory** for any change to a server interface or frontend feature:
  open the affected viewer in an external fullscreen Chromium-family browser, not VS Code's integrated browser, and confirm the feature works visually before marking work done.
- Record the browser window or display resolution used for manual visual validation whenever layout, rendering, or responsive behavior could affect the result.
- **Write Playwright end-to-end tests** for all browser-facing features and server interface changes.
  When executing browser-hosted frontend checks, first try the MCP Playwright/browser tools. Fall back to repo-local Playwright commands only when the MCP surface is unavailable or cannot cover the scenario.
  Shared managed-viewer suites live under `memory-viewers/viewer-api/viewer-api/frontend/dioxus/e2e/shared/`.
  Spec-viewer release E2E lives under `memory-viewers/spec-viewer/frontend/dioxus/`; run it with `npm run test:e2e:release`.
  Ticket-viewer release E2E lives under `memory-viewers/ticket-viewer/frontend/dioxus/`; run it with `npm run test:e2e:release`.
  Doc-viewer and log-viewer keep local Playwright wrappers under `tools/viewer/doc-viewer/e2e/` and `tools/viewer/log-viewer/e2e/`, importing shared suites from `memory-viewers/viewer-api`.
- For tracing-based tests, use:

<!-- rule-api:entry id=34054485-a4ff-47bf-b331-df68701cb967 slug=shared/agent-rules/quality-gates/l61 -->
```rust
let _tracing = init_test_tracing!(&graph);
```

<!-- rule-api:entry id=2d4b9e94-86e5-4abc-9d46-ad74c183b9f0 slug=shared/agent-rules/quality-gates/l65 -->
- If public behavior or docs changed, update the docs and run doc validation workflows.
- When dedicated test, doc, or cross-store-link tooling is missing or partial, use the strongest available command or manual check and call out the limitation explicitly in the status summary and spec traceability.
- Follow `.github/hooks/` reminders when they fire.
- Scratch notes belong in temporary files only; do not commit ephemeral notes.

<!-- rule-api:entry id=48ac3bab-a50d-40a7-9c8e-a452ae4c7f87 slug=shared/agent-rules/feedback-workflow/l70 -->
## Feedback Workflow

- Record feedback on canonical rule entries today. `spec-api` entries do not yet expose direct feedback tools or feedback summary fields.
- When feedback came from a specific generated spec or instruction surface, first locate the canonical rule entry that produced that text. Use `rule search` or `rule_list` / `rule_search` with `repo_scope`, `path_scope`, `section`, or `slug` filters until you have the exact rule ID or slug.
- Record the feedback on that rule entry with either:
  - CLI: `rule feedback <id-or-slug> --rating helpful|mixed|not-helpful [--note "..."] [--note-kind note|suggestion] [--session-id <id> --agent-or-user-id <id>]`
  - MCP: `rule_record_feedback` with `id`, `rating`, optional `note`, optional `note_kind`, and the `session_id` + `agent_or_user_id` pair when a manual session reference is needed.
- If you are reacting to a native spec entry rather than generated rule text, include the spec ID, path, and section in the feedback note text and open or update the corresponding spec or ticket work. Do not describe that as direct spec-entry feedback when the current storage is rule-entry scoped.
- Review follow-up queues with `rule list --low-rated-only`, `rule list --unresolved-only`, or the MCP `rule_list` / `rule_search` flags `low_rated_only` and `unresolved_only`.

<!-- rule-api:entry id=4f6e76bb-90d4-4d90-9e06-ca03d134ae67 slug=shared/agent-rules/escalation-rules/l80 -->
## Escalation Rules

- If blocked by ambiguity after focused research (10-15 minutes), ask the user.
- If evidence conflicts or architecture tradeoffs are required, ask before committing to a direction.
- If unexpected workspace changes appear that you did not make, stop and ask how to proceed.

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

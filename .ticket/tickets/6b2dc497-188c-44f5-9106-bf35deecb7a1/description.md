# session-cli + session-mcp: bootstrap surfaces

Expose the runtime session-context operations through CLI and MCP, headers-only (D6).

## Scope
- `session-cli` (`memory-api/tools/cli/session-cli`): add `init` (optional `--ticket-id`, resume-safe per D1/D9), `pin`, `unpin`, `view` alongside existing `check-in`/`lookup`/`query`/`peek-*`. Honor `--toon`/`--json`.
- `session-mcp` (`memory-api/tools/mcp/session-mcp`): add `session_init`, `session_pin`, `session_unpin`, `session_view` alongside existing tools.
- Surface end-of-session **rating** on pinned entities via the feedback-api CORE curation model (helpful/mixed/not-helpful + note).
- `view` returns short headers only; agents fetch full bodies via existing get/peek tools.
- Tests: CLI round-trip; MCP tools return structured results matching the frozen schema.

## Depends on
- Runtime session-context model (412964a3) and cascade (d8f76965).

## Spec
`memory-api/session-api/runtime-session-context` (709f067a).
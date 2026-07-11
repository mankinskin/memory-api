## Corrected scope (after investigation)
The original premise ("health_check has no CLI parity") is outdated. `ticket-api::health::collect_findings` already exists and a `ticket health` CLI subcommand, MCP `health_check`, and HTTP `/api/graph/health` all exist. The REAL problems are:

### 1. Health-finding logic is triplicated, not delegated
Each transport re-implements the finding generators instead of calling the shared `collect_findings`:
- memory-api/crates/ticket-api/src/health.rs (canonical)
- memory-api/tools/cli/ticket-cli/src/cli/commands/ops/health/findings.rs (duplicate)
- memory-api/tools/http/ticket-http/src/serve/handlers/graph/quality/findings.rs (duplicate)
- memory-api/tools/mcp/ticket-mcp/src/server/health/findings.rs (duplicate)
This causes drift (e.g. the `unblocked_with_deps` removal had to be applied in 4 places). Transports should delegate to the single `ticket-api` call.

### 2. Parameter surface differs across transports
- CLI HealthArgs: root, all, stdin, depth, direction, where_clauses (no `ids`)
- MCP HealthCheckInput: workspace, root, all, ids, depth, direction (no `where`/`stdin`)
- HTTP HealthCheckQuery: workspace, root, all, depth, direction (no `ids`/`where`)
Reconcile to a uniform option set (root, ids, all, depth, direction, where/filter) backed by one request/response shape.

## Acceptance criteria
- All three transports delegate health findings to `ticket-api::health::collect_findings` (single source of truth; no per-transport finding duplication).
- CLI, MCP, and HTTP expose the same parameters and identical output schema.
- Transport-parity test or documented manual verification per surface.

## Related
- The de-duplication should also fold in the cross-store dangling-edge fix (f3a58d3c) and the ready→ready removal (835b332b) so all three checks live once.
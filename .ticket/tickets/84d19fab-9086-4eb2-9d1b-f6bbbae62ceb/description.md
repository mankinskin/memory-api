# [ticket-mcp] Expose ticket move planning and execution over MCP

## Goal

Expose the move capability to MCP clients so agents can dry-run and apply safe workspace moves without shelling out to the CLI.

## Scope

Add MCP tools for:

- move preflight / dry-run
- move apply
- move resume / rollback (if surfaced separately)

Return structured fields for source store, target store, touched ticket references, path-reference rewrites, board blockers, and journal status.

## Acceptance criteria

- [ ] MCP callers can run a dry-run and receive the full move plan as structured data.
- [ ] MCP callers can execute and recover a move using the shared storage primitive.
- [ ] Error payloads preserve the fail-closed reasons from preflight.

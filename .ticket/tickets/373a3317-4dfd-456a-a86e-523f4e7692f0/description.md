# [ticket-http] Add ticket move endpoint for workspace relocation

## Goal

Expose the move capability over HTTP for remote tooling and UI-driven workflows.

## Scope

Add a typed endpoint for dry-run and apply, for example `POST /api/tickets/{id}/move`, backed by the same planner and storage primitive used by CLI and MCP.

The route should accept the target workspace root and dry-run/apply mode, and return structured move plans, blockers, journal identifiers, and recovery status.

## Acceptance criteria

- [ ] HTTP callers can preview and apply a supported move.
- [ ] HTTP responses preserve typed blocker reasons for unsupported topologies, dirty git worktrees, and active board claims.
- [ ] The route does not duplicate storage logic already implemented in ticket-api.

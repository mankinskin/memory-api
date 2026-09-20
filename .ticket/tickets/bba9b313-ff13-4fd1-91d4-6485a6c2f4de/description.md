## Objective

Add a dedicated query capability to the Session API that answers "which sessions worked on ticket X", using relation signals that already exist in the session data model but are not queryable today.

## Background

`SessionQuery` (memory-api/crates/session-api/src/store.rs#L92-L104) filters only on `session_id_prefix`, `conversation_id`, `agent_id`, `text`, `limit`. There is no ticket filter. `session_matches_query` (memory-api/crates/session-api/src/store/helpers/storage.rs#L438-L480) does not consult `SessionMetadata.ticket_id` or `SessionLinks.ticket_ids` at all, even though both fields already exist and are populated:
- `SessionMetadata.ticket_id: Option<String>` @ memory-api/crates/session-api/src/model.rs#L292-L306
- `SessionLinks.ticket_ids: Vec<String>` @ memory-api/crates/session-api/src/model/links.rs#L7-L15

This is a query-surface gap, not a data-model gap.

## Locked design decisions (from interview 04-08-2026)

- **API shape**: a separate dedicated entry point, `sessions_for_ticket(ticket_id, strength)`, NOT a new field on `SessionQuery`. Returns a purpose-built result type distinct from the existing session-listing result.
- **Relation-strength tiers** (caller picks a minimum tier):
  - `strict`: only `SessionMetadata.ticket_id == ticket_id` (the session's formal check-in ticket).
  - `linked`: `strict` OR `ticket_id` present in `SessionLinks.ticket_ids`.
  - `mentioned`: `linked` OR `ticket_id` present in the session's handoff package `target_tickets`.
  - Bare ticket-id substring scanning of `transcript.json` is explicitly OUT of scope — excluded as too noisy per the interview decision.
- **Result payload**: each returned row must carry: session id, agent id, start/end timestamps, git branch, worktree path, and which relation signal matched (`strict`/`linked`/`mentioned`). Do not inline the full handoff package body.
- Implement across all three surfaces: the session-api store/query layer, session-cli (memory-api/tools/cli/session-cli), and session-mcp (memory-api/tools/mcp/session-mcp).

## Acceptance Criteria

1. `session-api` exposes a `sessions_for_ticket(ticket_id: &str, strength: RelationStrength) -> Vec<TicketSessionMatch>` (or equivalent) query entry point, separate from `SessionQuery`, where `RelationStrength` has exactly the three tiers `strict`/`linked`/`mentioned` defined above.
2. The `strict` tier matches only on `SessionMetadata.ticket_id`; the `linked` tier additionally matches `SessionLinks.ticket_ids`; the `mentioned` tier additionally matches handoff package `target_tickets`. No tier scans `transcript.json` text.
3. Each result row includes session id, agent id, start/end timestamps, git branch, worktree path, and the matched relation-strength signal — verifiable via a unit test asserting all fields are populated for a fixture session.
4. `session-cli` exposes a command (e.g. `session sessions-for-ticket <ticket-id> --strength <tier>`) that calls the new entry point and prints the result rows.
5. `session-mcp` exposes an equivalent MCP tool with the same parameters and result shape, registered alongside the existing tools in memory-api/tools/mcp/session-mcp/src/server.rs.
6. Unit/integration tests cover all three strength tiers against fixture sessions with distinguishable `ticket_id`, `ticket_ids`, and handoff `target_tickets` values, asserting correct inclusion/exclusion at each tier boundary.
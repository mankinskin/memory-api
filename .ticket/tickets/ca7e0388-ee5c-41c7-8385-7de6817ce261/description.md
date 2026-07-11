## Capability (API landed)
`TicketStore::release_lease(ticket_id, requester)` added in memory-api/crates/ticket-api/src/storage/store/board.rs with the semantics:
- the holder (`working_by == requester`) may always release its own lease;
- any caller may release a stale (expired) lease;
- a live lease held by a different agent is rejected with `LeaseConflict`;
- releasing a ticket with no active lease is a no-op.
Unit test `release_lease_enforces_owner_and_stale_rules` covers all four cases (passing).

## Remaining work
1. Surface `release_lease` across all transports (parity): ticket-cli (`release`/rework `unclaim` to clear orphaned/stale leases even without a board entry), ticket-mcp tool, ticket-http route. Reconcile with the existing board check-in/out flow.
2. Fix the orphaned-lease bug that motivated this: a lease can outlive its board entry (observed on 82d6ada4 — `board check-out` reports "no active board entry" while `list_leases` still shows the lease). board_check_out should release the lease even when the board entry is already gone.
3. Clear the stale `copilot` lease on 82d6ada4-ac35-45a7-9df6-7b7501d58e70 (root store) once a transport surface exists.

## Acceptance criteria
- release_lease is invokable from CLI, MCP, and HTTP with identical semantics.
- board check-out never leaves an orphaned lease.
- The 82d6ada4 orphaned lease is cleared.
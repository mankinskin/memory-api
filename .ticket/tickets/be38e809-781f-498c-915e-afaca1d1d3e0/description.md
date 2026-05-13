# [Board][Validation] Concurrent Check-In, Crash Recovery, and Cross-Interface Consistency

## Objective

Validate that the draftboard behaves correctly under the failure modes and concurrency patterns it is explicitly meant to manage.

The current track includes acceptance-criteria-level tests inside the API, CLI, MCP, and integration tickets, but there is no dedicated validation owner for cross-interface races, restart recovery, or stale-entry behavior under realistic failure conditions.

## Why This Needs Its Own Ticket

The draftboard is a synchronization feature. If it fails, it will fail under concurrency, crash recovery, or interface mismatches:
- two agents check into overlapping files at nearly the same time
- CLI and MCP return subtly different semantics for the same operation
- a session crashes after check-in but before meaningful progress
- persisted board data survives a restart but the agent does not
- stale entries remain forever because cleanup is too cautious, or disappear too early because cleanup is too aggressive

These issues are hard to reason about from isolated unit tests alone.

## Validation Scope

### Concurrency and atomicity
- Two simultaneous `board_check_in()` calls for overlapping files
- Two simultaneous `board_check_in()` calls that hit the WIP limit boundary
- `board_show()` racing with `board_heartbeat()`
- `board_clean()` racing with `board_heartbeat()` or `board_check_out()`

### Crash and restart recovery
- Orchestrator/process restart with persisted active entries in redb
- Restart after entries have already crossed the stale threshold
- Restart after a completed entry is inside vs outside the audit window
- Recovery when lease state and board state disagree

### Cross-interface consistency
- CLI `ticket board ...` and MCP `board_*` tools return the same semantics and core fields
- `ticket next`, `ticket status`, and MCP `next_tickets` surface the same WIP, stale, and exclusion behavior
- `board_show(agent_id=...)` performs read-only snapshot plus heartbeat consistently across CLI and MCP

### Stale-entry mitigation
- High-priority stale warnings appear after one hour
- Renew path clears stale status correctly
- Explicit cleanup path only removes entries after the intended review/confirmation semantics
- Stale entries that still own files block new conflicting work until explicitly resolved

## Resolved Decisions (2026-04-12)

Resolved via structured interview (`agents/interviews/20260409_INTERVIEW_DRAFTBOARD_REFINEMENT.md`, Batch 7).

### Validation Priorities

- **Q19 → E (All four race conditions):** Overlapping-file simultaneous check-in, WIP-limit boundary races, `board_show()` vs `board_heartbeat()`, and `board_clean()` vs `board_check_out()`/`board_heartbeat()` all require automated tests before the feature ships. Skipping any of them means discovering coordination failures in production under exactly the conditions the draftboard is designed to protect against.
- **Q20 → B (Persisted entries recovered, stale recomputed):** On restart, all persisted board entries are loaded from redb and stale status is recomputed from current time vs last heartbeat. The board is the source of truth (per Q7); strict cross-reconciliation against leases or ticket state is deferred because leases are internal mirrors and ticket state is advisory.
- **Q21 → B (Same semantics and same core fields):** CLI and MCP must return the same semantic meaning for every board operation, and core response fields (entry IDs, status, file lists, stale flags, confirmation tokens) must match. Golden parity (exact formatting match) is deferred as brittle. A shared integration test harness exercising the same workflow through both interfaces and asserting structural field parity is the recommended testing shape.

## Deliverables

- Validation matrix covering API, CLI, MCP, and `next`/`status`
- Concurrency test suite for check-in / heartbeat / cleanup races
- Restart recovery scenarios with persisted board state
- Cross-interface golden outputs for key workflows
- Manual validation checklist for stale-entry mitigation and operator review

## Acceptance Criteria

- [x] Validation matrix covers concurrency, restart recovery, and cross-interface consistency
- [ ] Automated tests exist for overlapping-file check-in races and WIP-limit races
- [ ] Automated tests exist for restart recovery with persisted active and stale entries
- [x] CLI and MCP outputs are verified to match semantically for all board operations
- [ ] `next` / `status` / MCP `next_tickets` are verified to surface stale and excluded entries consistently
- [ ] Stale-entry renewal and explicit cleanup paths are validated end to end
- [ ] Failure cases produce actionable, non-destructive behavior rather than silent corruption or silent cleanup

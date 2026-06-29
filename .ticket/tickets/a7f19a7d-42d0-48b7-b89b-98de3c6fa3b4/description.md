# Refresh cross-domain move matrix and add a move benchmark now that the kernel landed

## Goal

The generic move kernel (`0a510279`) has landed, but the cross-domain operation matrix (`memory-matrix`) still reports `move` as **blocked-with-reason** and its test asserts the blocked state. Update the matrix to actually exercise move via the kernel and add real latency coverage.

## Problem / current state

- `memory-api/crates/memory-matrix/src/lib.rs` `move_op` is hardcoded:
  `blocked("generic move kernel (ticket 0a510279) not yet landed; cross-worktree move is blocked-with-reason until it lands")`.
- `memory-api/crates/memory-matrix/tests/matrix.rs::move_cells_are_blocked_with_a_reason` asserts **every** domain's `move` cell is `Blocked` "until the move kernel lands".
- `memory-api/crates/memory-matrix/budgets.toml` defines `move = 1000` (ms budget) that is never exercised because the cell is blocked.
- Net effect: the matrix now misreports reality — the kernel exists and at least ticket + spec moves work.

## Scope

- Replace the hardcoded `move_op` block with a real cross-worktree move exercised through `move_kernel` for the domains that have a `MoveDomain` impl (ticket, spec today), recording a `Passed` execution with measured duration against the `move = 1000` budget.
- Keep domains without a move adapter as `Blocked` with an accurate reason (not the stale "kernel not landed" text).
- Update `move_cells_are_blocked_with_a_reason` to assert per-domain expectations (passed for adapter-backed domains, blocked-with-accurate-reason otherwise).
- Add a Criterion move benchmark (or wire move into `operation_matrix`) measuring preflight + execute + rollback so the `move` budget has real evidence.

## Acceptance criteria

- [ ] No matrix cell references "ticket 0a510279 not yet landed".
- [ ] Adapter-backed domains record a `Passed` move execution with a measured duration checked against the budget.
- [ ] A move benchmark produces latency evidence recorded via test-api.

## Relationship / traceability

- Depends on `0a510279` (generic move kernel).
- Touches the cross-domain matrix owned by `751f0e71` / `03ed4121`; coordinate with that track before editing `memory-matrix` (it has an active board entry).
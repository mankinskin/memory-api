## Problem — data loss in production use

`TestStoreConfig::record_execution` (memory-api/crates/test-api/src/store.rs) unconditionally calls `self.prune_execution_runs(2)` after every write. This function computes, across **every** execution file in the store (regardless of which validation spec or ticket it belongs to), the most-recent `executed_at` per distinct `provenance.run_id`, keeps only the **2 globally newest run_ids**, and **deletes every execution file** whose `run_id` is not in that top-2 set.

`provenance.run_id` is mandatory (`record_execution` enforces `validate_interoperability_contract`, which rejects a missing `run_id`), so every execution participates in this pruning.

Consequence: recording a *new, unrelated* execution with a fresh `run_id` silently deletes evidence for *other, unrelated* tickets/specs whenever more than 2 distinct `run_id`s exist in the store — with no confirmation, no dry-run, no scoping by ticket/spec, and no warning surfaced to the caller.

## Reproduced this session (real data loss occurred)

While recording validation-execution evidence for tickets a7601cb7/b4954d6c/16e112a7 (PROV/INGEST/MAP) using distinct `run_id`s per call, each subsequent `test_record_execution` call silently deleted the previous call's execution file, and also deleted the already-recorded BFS (3fa60398) and SYNTH (3d4c4739) execution evidence. A second wave deleted 6 git-tracked, previously-committed execution files belonging to ticket fb6aa078 (DESIGN): `exec-val-feedback-api-{ring-persistence,store-roundtrip,transport-surface}-20260712{,-ring-redistribution}.json`. Both incidents were recovered (the git-tracked files via `git restore`; the uncommitted ones by re-running the underlying test suites and re-recording). The recovery required consolidating all newly-recorded executions onto one shared `run_id` (`run-review-remediation-20260714`) to avoid a further cascade — this is a workaround, not a fix, and the store still holds 3 distinct `run_id`s today (one over the keep-2 threshold), so the **next** unrelated write with a new `run_id` will delete one of the two remaining legacy runs.

## Why this matters

The `.test` store is documented (test-mcp server instructions) as durable evidence linked from tickets/specs for review and traceability. A retention policy that globally caps distinct `run_id`s at 2 and deletes everything else — silently, on every write, with no ticket/spec awareness — is fundamentally incompatible with that purpose. It appears designed for a different use case (e.g. capping churn from a single repeatedly-invoked CI harness that reuses one `run_id` across a whole suite), not for many independent, long-lived, ticket-linked one-off evidence records.

## Fix direction (needs a design decision, not just a patch)

Options to evaluate:
1. Scope pruning by `validation_spec_id` (or by ticket) instead of globally across the whole store, so unrelated evidence never competes for the same retention slots.
2. Make pruning opt-in (a flag on `record_execution`, defaulting to off) rather than an unconditional side effect.
3. Increase/parametrize `keep_runs` and expose it via `TestStoreConfig`/MCP tool input instead of the hardcoded `2`.
4. Never prune executions that carry `links.ticket_ids` (durable ticket evidence) — only prune anonymous/unlinked harness runs.

Whatever direction is chosen, add a regression test that records 3+ independent ticket-linked executions with distinct `run_id`s and asserts none of them are silently deleted.

## Acceptance Criteria

- Recording a new validation execution never silently deletes ticket-linked evidence for an unrelated ticket/spec.
- A regression test exists proving 3+ independent ticket-linked executions with distinct run_ids all survive.
- Existing intentional pruning behavior (if kept for a narrower use case) is documented and scoped so it cannot cross ticket/spec boundaries.
- The current 3-way `run_id` overhang in `memory-api/.test` (`run-review-remediation-20260714`, `feedback-rebuild-20260712`, `ring-redistribution-20260712`) is resolved (e.g. by consolidating the two legacy runs or by fixing the scoping) so no further silent deletion is pending on the next write.

## Evidence

Discovered and reproduced live during review of tickets a7601cb7 (PROV), b4954d6c (INGEST), 16e112a7 (MAP), 3fa60398 (BFS), 3d4c4739 (SYNTH) evidence recording. Related to 905d05ae (test-mcp record/list store routing split) but distinct: that ticket is about which store a read/write resolves to; this ticket is about writes to the *correct* store destroying *other* evidence already in it.

## Decision (2026-07-14 review) — chosen fix direction

Adopt **Option 1: scope pruning by `validation_spec_id`**. Rationale grounded in a fresh read of the code:

- `prune_execution_runs` (memory-api/crates/test-api/src/store.rs, ~L470-L521) currently keys retention on `run_id` alone, globally across the whole store, and `record_execution` (~L133) calls it unconditionally on every write. Change the retention key from `run_id` to the composite `(validation_spec_id, run_id)` and apply the keep-N window independently per spec.
- This eliminates the cross-ticket/cross-spec deletion (the real defect): the feedback-ring specs (PROV/INGEST/MAP/BFS/SYNTH plus DESIGN's specs) each occupy their own retention namespace, so recording one spec's execution can never delete another spec's evidence.
- It preserves the existing intentional churn-cap without touching that test: `record_execution_keeps_only_newest_two_runs` (memory-api/crates/test-api/src/store/tests.rs, ~L179-L215) records three runs of a *single* spec `vt-a`; per-spec scoping still keeps only the newest 2, so run1 is still pruned and the test stays green and meaningful.
- Rejected alternatives: opt-in flag (option 2) and keep-count bump (option 3) both leave the global blast radius intact; never-prune-ticket-linked (option 4) becomes redundant once scoping is per-spec.

Regression test to add (satisfies AC#2): record 3+ executions across 3 distinct `validation_spec_id`s with 3 distinct `run_id`s and assert all three survive `record_execution`.

AC#4 (the current 3-way `run_id` overhang) dissolves automatically once scoping is per-spec, because the overhanging runs belong to different specs and no longer compete for a shared global keep-2 budget.
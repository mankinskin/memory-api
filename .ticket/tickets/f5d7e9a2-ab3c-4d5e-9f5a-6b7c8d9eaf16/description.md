# [bootstrap][T6] Verify Merge and Completion Linkage with Assignment Chain

## Context

After validation passes (T4), the ticket advances through release gates toward merge. The merge record must be fully traceable: it must include the worker assignment_id, the validator assignment_id, the merge commit, and the release target. This is the final integration test in the T1–T6 suite — it validates the complete end-to-end audit chain from dispatch through merge.

## Completion Path

```
T4 validation passed
  ↓
ticket → release-candidate
  ↓
release gates checked:
  R1: all included tickets validated
  R2: no open sev0/sev1 bugs in scope
  R3: migration notes present if schema/state changed
  R4: release smoke suite passes
  ↓
branch merged to merge target
  ↓
ticket updated: released → monitoring → done
```

## Merge Record Requirements

The merge event must include:
- `worker_assignment_id` — from the implementation phase
- `validator_assignment_id` — from the validation phase
- `merge_commit` — the resulting commit SHA
- `release_target` — version or milestone label

This is enforced by `finalize-merge` (currently a Phase 2 stub in the CLI — this ticket does not require it to be fully implemented, only that the merge record contains the required fields).

## Release Gate Coverage in This Test

For the integration test, a **simplified gate check** is sufficient:
- R1: ticket is in `validated` state (gated by T4)
- R2: no sev0/sev1 bug tickets linked (verify by query)
- R3: not applicable (no schema change in test scenario)
- R4: smoke suite = the T1–T6 test suite itself passing

R3 and R4 full production enforcement is out of scope for this ticket.

## Scope

### What is being built / tested
- Simulate validation passed → release-candidate → merge flow
- Merge record structure validation: all required fields present
- Assignment chain linkage: merge record traces back to both worker and validator assignment_ids
- Ticket state after merge: transitions to `released` (or `done` in test scenario)
- Audit query: given a merge commit, retrieve the full assignment chain

### Not in scope
- Full release gate enforcement (production-grade R1–R5 policy)
- Remote push to GitHub (AOH ADR-3 — explicit operator action only)
- Rollback execution

## Acceptance Criteria

- [ ] Ticket advances from `validated` → `release-candidate` after gate R1 + R2 pass
- [ ] Branch is merged to merge target; merge commit is captured
- [ ] Merge record includes: worker_assignment_id, validator_assignment_id, merge_commit, release_target
- [ ] Ticket transitions to `released` (or `done`) after merge
- [ ] Audit query for the merge commit returns the complete assignment chain (worker + validator)
- [ ] T1–T6 full sequence passes as a CI signal (Required exit criterion for the host executor bootstrap)

## Dependencies

- Depends on: T4 (`d3b5c7e9`) — validation pass is the entry condition
- T5 (`e4c6d8f1`) — parallel, independent; both must pass for CI exit
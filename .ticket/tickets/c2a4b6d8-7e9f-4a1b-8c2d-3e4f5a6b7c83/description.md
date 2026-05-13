# [bootstrap][T3] Validate Ticket Lifecycle Happy Path Under Executor

## Context

Once a worker is authenticated (T1) and context-verified (T2), it proceeds through the core ticket mutation lifecycle: claim → implement → attach evidence → unclaim. Every event in this lifecycle must carry the `assignment_id` so the full audit chain from coordinator dispatch to completion is traceable.

This ticket covers the happy-path integration test for this lifecycle.

## Lifecycle Under Executor

```
coordinator dispatches assignment packet
  ↓
worker authenticates (T1)
worker reports branch + cwd (T2)
  ↓
worker claims ticket (assignment_id attached to claim event)
  ↓
worker updates ticket fields / state (assignment_id on all update events)
  ↓
worker attaches evidence refs (test results, cargo check output, diffs)
  ↓
worker unclaims ticket (final handoff payload, assignment_id on unclaim event)
  ↓
coordinator receives handoff and advances ticket to review/validating
```

## assignment_id Audit Requirements

`assignment_id` must appear on:
- Lease/claim events
- All `task_update` progress events
- Evidence attachment events
- Unclaim/handoff event
- Any linked bug creation events (not in this ticket's scope)

## Evidence Reference Format

Evidence refs attached by the worker during this flow:
```
EvidenceRef {
    kind:    TestResult | CargoCheck | DiffSnapshot | Screenshot
    path:    relative path within the archive artifact directory
    summary: human-readable one-liner
}
```

## Scope

### What is being built / tested
- Claim with assignment_id propagation
- Structured ticket field updates with assignment_id on each event
- Evidence attachment API
- Unclaim with final handoff payload
- Verification that all events carry assignment_id

### Not in scope
- Validation handoff to a second agent (T4)
- Early-stop/failure paths (T5)
- Merge and release linkage (T6)

## Acceptance Criteria

- [ ] Worker claims ticket; claim event contains `assignment_id`
- [ ] Worker updates ticket state and fields; each update event carries `assignment_id`
- [ ] Worker attaches at least one evidence ref with kind, path, and summary
- [ ] Worker unclaims ticket with handoff payload; unclaim event carries `assignment_id`
- [ ] After unclaim, coordinator can query all events for this assignment and see a complete audit chain
- [ ] Test runs without network calls to real LLM provider (stub provider)

## Dependencies

- Depends on: T1 (`a8d6c1d2`), T2 (`b1f3e2a4`)
- Blocks: T4 (validation handoff begins where T3 leaves off)
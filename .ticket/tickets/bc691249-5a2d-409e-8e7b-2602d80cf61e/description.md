# [ticket-api] Add journaled storage-layer execution for cross-workspace ticket moves

## Goal

Execute a supported move safely at the storage layer, with resumable journal state and rollback when a step fails.

## Scope

Implement the mutation engine that:

- acquires source/target store locks and a ticket-level move lock
- persists a move journal capturing the plan, touched files, and recovery steps
- relocates the ticket folder into the target store using git-aware file movement when available
- reindexes or scans the source and target stores after the move
- validates that the moved ticket resolves from the target store and disappears from the source store's owned set
- supports resume/rollback from an interrupted journal state

This ticket does **not** own transport surfaces; it owns the reusable move primitive.

## Acceptance criteria

- [ ] A partially failed move can be resumed or rolled back without leaving the ticket folder owned by two stores.
- [ ] Source and target indexes are reconciled after execution and after rollback.
- [ ] The journal captures enough state to explain what happened and what recovery step to run next.

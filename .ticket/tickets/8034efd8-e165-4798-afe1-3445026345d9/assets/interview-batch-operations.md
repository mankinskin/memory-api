# Interview: Batch Operations

**Date:** 2026-04-08
**Applies to:** Epic `35a6d14b` (new ticket needed)

## Question

Do you want multi-select and batch operations (e.g., bulk state transition, bulk field update, bulk close)? Or keep it strictly one-ticket-at-a-time for the initial port?

## Answer

**Yes, we want to support batch operations: multi-select, queue and batch apply, bulk field updates, filter result bulk updates.**

## Implications

- **New ticket required** — batch operations are not scoped in any existing ticket
- Multi-select UI: checkboxes in TicketTree, select-all, range-select (shift+click)
- Batch action bar: appears when multiple tickets selected, offers bulk state transition, field update, close/cancel
- Queue model: batch operations queued and applied sequentially with progress indicator
- Filter-based bulk: "Apply to all matching filter results" (e.g., close all tickets matching a search)
- Backend support: ticket-api `batch` command already supports transactional multi-command execution
- ticket-http needs a batch endpoint (POST /api/batch) or the frontend queues individual mutations
- Optimistic UI: show batch progress with per-ticket status (success/fail), rollback on batch failure
- Undo support: batch operations should be revertible (at minimum, show what was changed)

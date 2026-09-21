# Objective
Drive the full 2026-07-05 audit baseline from 551 findings toward near-zero through ordered category execution and bounded implementation batches.

# Baseline Snapshot
- ticket_graph: 258
- file_length: 182
- static_complexity: 108
- compiler_warning: 1
- test_execution: 1
- coverage: 1
- source: target/tmp/audit-full-2026-07-05.json

# Execution Order
1. ticket_graph category
2. stability category (compiler warning, test execution, coverage)
3. static complexity category
4. file length category

# Session Loop For Every Batch
1. Check in to the active batch ticket on board.
2. Confirm scope from the audit artifact and category target counts.
3. Implement only the current batch scope.
4. Re-run focused validation and then full audit summary by category.
5. Record delta counts and unresolved blockers in the batch ticket.
6. Move batch to in-review only when acceptance criteria are met.

# Acceptance Criteria
- Every child category ticket has all child batches closed in dependency order.
- Each category documents before and after counts plus residual findings.
- No new audit finding class is introduced by remediation.
- A final full audit run is attached showing aggregate reduction from baseline.

# Risks And Controls
- Risk: broad refactors inflate scope.
  Control: strict batch boundary; defer cross-batch edits unless blocker.
- Risk: count drift between runs.
  Control: always compare against this baseline and latest full report.
- Risk: regressions while splitting large files.
  Control: compile and test gate after each split set.

# 2026-07-07 linked resolution note
- Retro-cleanup hardening completed under `528af270` and linked stream `e7c593dd`.
- Removed stale retro/deleted dependency edges and reconciled scan state.
- Memory-api workspace policy + empty UUID artifact pruning landed; memory-api force scan now reports zero diagnostics for the prior residual class.
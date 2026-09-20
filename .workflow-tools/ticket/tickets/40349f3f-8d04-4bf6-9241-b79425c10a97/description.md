## Problem

`sessions_for_ticket` (spec `e5f8a2c1`) answers "which sessions worked on this ticket" via three cumulative tiers strict ⊆ linked ⊆ mentioned, using ONLY structured signals (never transcript text). The strict tier reads `SessionMetadata.ticket_id`, which is written by `check_in_worktree`.

A dry-run backfill against the real store of 231 sessions measured:
- `branch` populated: 0/231
- `worktree_path` populated: 0/231
- linkage recoverable only via handoff `target_tickets`: 37 associations across 4 sessions (~1.7%)

Root cause: the Copilot capture hook (`memory-api/crates/session-api/src/bin/copilot-capture-hook.rs`) creates sessions passively on every turn and never calls `check_in_worktree`. Branch and worktree path are therefore never recorded for the vast majority of sessions, and `ticket_id` has nowhere to come from.

## Fix (this ticket)

Added `SessionStoreConfig::infer_worktree_from_environment` (`memory-api/crates/session-api/src/store/config/worktree_capture_inference.rs`), wired into the capture hook after every transcript capture. It:
- resolves the current git branch and worktree root via `git rev-parse`,
- reuses the backfill's short-id parser/resolver (`parse_agent_branch_short_id`, `resolve_ticket_prefix`) to check the branch against the ticket store,
- only writes when no worktree assignment already exists (never overwrites a real `check_in_worktree`),
- never writes an unresolved ticket id,
- is fully best-effort: any resolution failure is swallowed with an `eprintln!` warning so capture never fails.

## Linked

Root cause of ticket `2b75bac2-ff14-43c3-8e87-1e801772f309` (sessions_for_ticket returns nothing).


## Follow-up (2026-08-06): the capture hook must redirect to the MAIN session store

`infer_worktree_from_environment` records WHICH worktree a session is working in. It does not fix WHERE the session record itself is written, and that is a separate defect exposed by the session-anchored resolution work in `fa2ba34b`.

### Problem

`memory-api/crates/session-api/src/bin/copilot-capture-hook.rs#L66` resolves the session store root from the current working directory. When an agent works inside `.worktrees/<name>`, the hook fires with that worktree as cwd and writes the session record into the WORKTREE's `.session` store.

This is wrong in the redirect model the proxy assumes:

- A session is a property of the developer/agent, not of a checkout. It outlives any single worktree, and `SessionWorktreeAssignment.allocation_mode = Rotated` explicitly anticipates a session moving between worktrees.
- The proxy in `fa2ba34b` must resolve `session_id` -> active worktree. If session records are scattered across per-worktree stores, there is no single store to resolve against, and the resolver would have to already know the worktree in order to find the session that tells it the worktree.
- `.session` is version-controlled, so every worktree gets its own copy — the same structural divergence that `fa2ba34b` documents for `.ticket`.

### Required behavior

The session record lives in the MAIN checkout's store while the WORK happens in the assigned worktree. The hook must record the worktree as *data on the session*, not as the *location of the session*.

## Additional Acceptance Criteria

- AC-R1: The capture hook resolves its session-store root to the main checkout even when invoked with a linked worktree as cwd. Resolution uses an authoritative redirect (for example the common git dir, or an explicit configured main-store root), not `std::env::current_dir()`.
- AC-R2: The worktree the hook was invoked from is still recorded, as the session's worktree assignment — the redirect must not lose the worktree signal that this ticket already added.
- AC-R3: Invoking the hook from a linked worktree creates or updates exactly one session record, in the main store, and creates no `.session` entry inside the worktree.
- AC-R4: Resolution remains best-effort and non-fatal, consistent with the existing contract: a failure to resolve the main store must warn and degrade, never fail the capture.
- AC-R5: A test invokes the hook with cwd set to a linked worktree and asserts both the main-store write and the absence of a worktree-local session write.

Related: `fa2ba34b` (session-anchored MCP workspace resolution) depends on session records being resolvable from one authoritative store.

## Correction (2026-08-06): anchor on the WORKTREE, not the main store

The "Follow-up (2026-08-06)" section above and its AC-R1 through AC-R5 are SUPERSEDED. They specified redirecting capture to the main checkout's session store. The decided model is the opposite.

### Decided model

All active stores are worktree-local. `.session`, `.ticket`, and `.spec` live inside the session's worktree. The main checkout holds no active store; its copies are a merge target that becomes current only when a branch merges.

This is bootstrappable because worktree initialization is the FIRST action of a chat session: a chat lifecycle hook creates and initializes the worktree before any other tool runs. There is no interval in which a session exists without a worktree, so no main-checkout fallback is needed and none is specified.

### Corrected acceptance criteria

These REPLACE AC-R1 through AC-R5.

- AC-C1. `memory-api/crates/session-api/src/bin/copilot-capture-hook.rs#L66` no longer resolves the store root from `std::env::current_dir()`. It resolves from the session's active worktree assignment.
- AC-C2. Capture invoked while the process working directory is the main checkout, with the session assigned to a linked worktree, writes the session record into THAT worktree's `.session` store.
- AC-C3. The same invocation writes nothing into the main checkout's `.session`.
- AC-C4. When no worktree assignment resolves, capture warns and degrades. It does NOT fall back to the main checkout, and it does not fail the capture.
- AC-C5. A regression test drives the main-checkout-cwd / worktree-assignment split and asserts both the worktree write and the untouched main store.

### Traceability

- Spec `09f96d83-4795-4f19-9259-64ad0d452387` — section "Worktree-Anchored Capture (2026-08-06 refinement)" carries the corrected contract.
- Spec `aff42efb-422b-4abc-8a6c-cd176a3d0d5d` — the cross-server protocol applying the same anchoring at the MCP boundary.
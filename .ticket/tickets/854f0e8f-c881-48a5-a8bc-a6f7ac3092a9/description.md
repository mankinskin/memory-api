# [Board] Draftboard — Workspace WIP Coordination for Concurrent Agents

## Objective

Provide a workspace-global, short-term "daily planning board" that tracks the current state of all active work across concurrent agent sessions. The draftboard fills the gap between ephemeral user prompts and the persistent ticket database — it is the synchronization point where agents discover what is currently in flight, who owns which files, whether WIP limits are exceeded, and what needs human attention.

## Problem Statement

Today, multiple agents operate on the same ticket workspace simultaneously. Each agent starts a session, runs `ticket next` to pick work, and proceeds independently. There is no shared view of:

- **What is actively being worked right now** — tickets may be in `in-implementation` without any agent actually touching them
- **Who owns which files** — two agents can start editing the same files, leading to merge conflicts
- **How many concurrent tasks are running** — there is no WIP limit; agents can pile up unbounded parallel work
- **Which work items are stale** — an agent session may crash or be abandoned, leaving phantom "active" state
- **What a new agent session should focus on** — fresh sessions must re-derive context from scratch

The existing lease system (`claim`/`unclaim`) provides a low-level reservation mechanism but lacks WIP budgeting, file ownership tracking, stale detection, conflict resolution, and a summary snapshot for onboarding new sessions.

## Design Principles

1. **Minimal and ephemeral** — the board is a thin coordination layer, not a second ticket database. Entries are short-lived.
2. **Lock-gated reads** — `board show` acquires a read lock and returns an atomic, consistent snapshot. New agent sessions call this once to orient.
3. **Human escalation** — file ownership conflicts are flagged for human resolution rather than auto-resolved.
4. **WIP discipline** — configurable maximum concurrent entries; `board check-in` rejects new work when the limit is reached.
5. **Stale detection** — entries without heartbeats past their TTL are flagged as stale and surfaced prominently.
6. **Coexists with leases** — board entries internally claim leases for backward compatibility but add file ownership, WIP limits, and conflict detection.

## Fresh-Review Follow-Ups

This epic gained three additional child tickets after a second-pass refinement review:

- `84ceb9ce` — entry identity, resume flow, and synchronization invariants
- `c3143e3c` — stale-entry review, cleanup approval, and conflict resolution workflow
- `be38e809` — concurrency, restart recovery, and cross-interface validation

These tickets cover the first failure modes likely to appear once the draftboard is implemented: entry/key collisions during reuse, drift between board state and leases/ticket state, user-permission flow for cleanup, and race/restart behavior under real concurrent use.

## Dependency Graph

```
Design (8aff39cb)
    ↓
API Layer (0db86ac1) ← depends_on Design
    ↓
┌───────────┼───────────┐
CLI (bcc111c6)  MCP (ec52f7cb)  next/status Integration (74160bb8)
```

## Acceptance Criteria

- [ ] All child tickets completed
- [ ] `board show` provides a complete, lock-gated workspace status snapshot
- [ ] `board check-in` / `board check-out` track active agent work with file ownership
- [ ] WIP limits enforced across all check-in paths (CLI, MCP)
- [ ] Stale entries flagged with prominent warnings
- [ ] File ownership conflicts escalated to human
- [ ] `ticket next` and `ticket status` incorporate draftboard state
- [ ] MCP tools expose all board operations for agent sessions
- [ ] Identity/reuse invariants, cleanup approval flow, and validation coverage are finalized via the follow-up tickets
- [ ] Documentation updated for agent onboarding workflow

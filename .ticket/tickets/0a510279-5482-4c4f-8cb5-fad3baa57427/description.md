# [memory-api] Generalize cross-workspace move into a domain-neutral kernel with per-domain trait specialization

## Goal

Promote the proven ticket-only cross-workspace move (delivered by `505b2cd4`) into a **domain-neutral generic move kernel** in `memory-api` that every domain store (ticket, spec, rule, audit, session, feedback) reuses. Domain-specific behavior is injected through traits implemented in each domain api crate, so all domains gain the same safe move featureset with **no duplicated move logic (DRY)**.

## Problem / current state

The cross-workspace move contract is implemented directly against `TicketStore` and ticket-shaped types:

- `memory-api/crates/ticket-api/src/storage/move_planner.rs` — `impl TicketStore::plan_move_preflight`, `MovePreflightReport` (ticket-typed fields: `ticket_id`, `inbound_related_ticket_ids`, `outbound_related_ticket_ids`, `source_ticket`/`target_ticket: IndexedTicket`), `MovePreflightBlocker`.
- `memory-api/crates/ticket-api/src/storage/move_execution.rs` — `impl TicketStore::{execute,resume,rollback}_move_with_journal`, `MoveJournal`, `MoveExecutionPhase`, board-row migration, path-reference rewrite, lock/journal handling.

The v1 ticket move ticket (`505b2cd4`) deliberately scoped this to **ticket-only** ("Do not generalize to arbitrary memory-api entities until the ticket move contract is proven"). That contract is now in review, so the generalization follow-up is warranted. No dedicated generic-move ticket exists yet.

## Scope

- Extract a generic move kernel (read-only preflight planner + journaled/resumable/rollbackable executor) into shared `memory-api` storage, parameterized over an entity-agnostic core.
- Define trait injection / specialization points that each domain api crate implements to supply domain-specific behavior:
  - entity identity + on-disk path resolution for source/target stores
  - reference & edge enumeration (inbound/outbound) and destination visibility checks
  - board entry / lease detection and historical-row migration (where the domain has a board)
  - path-reference scan + rewrite policy for tracked text files
  - blocker classification mapping onto a shared blocker enum
- Migrate the existing ticket move onto the new kernel as the **first adopter**, preserving current behavior and tests.
- Ensure all domains can opt into the generic store move featureset through the shared kernel without copying logic.

## Non-goals

- Implementing cross-git-worktree / submodule move itself — that capability is owned by `21e6c015`; this ticket only ensures the generic kernel exposes it through trait specialization rather than reimplementing it.
- Changing the fail-closed board/lease policy or the journaled atomicity model.
- Cross-store transactional move (still out of scope; journaled + resumable remains the model).
- New surface (CLI/MCP/HTTP) behavior beyond exposing the generalized kernel; domain surfaces are wired in their own child tickets if needed.

## Acceptance criteria

- [ ] A domain-neutral move planner + journaled executor lives in shared `memory-api`, with no ticket-specific types in the kernel signatures.
- [ ] Trait specialization points are defined and documented for: entity/path resolution, reference & edge enumeration + visibility, board/lease checks + historical migration, path-reference rewrite, and blocker mapping.
- [ ] `ticket-api` move (`move_planner.rs` / `move_execution.rs`) is reimplemented on top of the generic kernel via the trait impl, with existing move tests still passing (planner preflight, execute/resume/rollback, active-board-entry fail-closed, historical board migration, path-reference rewrite + rollback).
- [ ] At least one additional domain (e.g. `spec-api` or `rule-api`) demonstrates the kernel is reusable by implementing the trait, even if surface wiring lands in a follow-up.
- [ ] No move logic is duplicated across domain crates — domain crates contain only their trait impls and domain-specific helpers.

## Relationship / traceability

- Follows and depends on `505b2cd4` ([ticket-api] Deliver safe cross-workspace ticket move for git-backed stores) — the proven ticket-only contract this generalizes.
- Planning lineage: `13e9ce28` (move planning).
- Cross-store context (these live in the **default/context-engine `.ticket` store**, so they are referenced textually rather than via graph edges — edges cannot cross stores):
  - `2b1279bd-c42f-4b0e-8835-d0d645a733ab` — Neutral storage kernel API migration (neutral shared storage/index/search symbols). The generic move kernel should build on the neutral kernel naming once available.
  - `671d4e47-b53d-4a04-aa1d-30f2aa8a2bbe` — multi-store architecture tracker.

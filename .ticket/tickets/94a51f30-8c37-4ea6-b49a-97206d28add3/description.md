# Adopt the generic move kernel across domains and expose move on all transports

## Goal

Now that the domain-neutral move kernel (`0a510279`, `memory_api::storage::move_kernel`) has landed and is proven by the `ticket-api` adapter plus a `spec-api` demonstration adapter, make cross-workspace move a first-class, surfaced capability for **every** domain store and **every** transport.

## Problem / current state

- Only `ticket-api` exposes move end-to-end (CLI `ticket move`, MCP `move_preflight/apply/resume/rollback`, HTTP move endpoint).
- `spec-api` implements `MoveDomain` (`SpecMoveDomain`, `SpecStore::{plan_move_preflight, execute/resume/rollback_move_with_journal}`) but has **no CLI/MCP/HTTP wiring** and the adapter is a minimal demo: it only enumerates `EntityStore::list_all_edges` and does **not** relink spec hierarchy (parent/child slugs), `code_ref` paths, or fulfillment links, nor rebuild the slug index after a move.
- `rule-api`, `audit-api`, `session-api`, `test-api`, `log-api`, `doc-api` have **no** `MoveDomain` impl at all.

## Scope

- Implement `MoveDomain` for each domain that should support move (`rule-api`, `audit-api`, `session-api`, and any other domain with a folder-per-entity store), reusing the kernel; no duplicated move logic.
- Harden the `spec-api` adapter to production quality: enumerate ALL spec references (hierarchy + code refs + fulfillment), relink them on move, and rebuild the slug index post-move; confirm `EntityStore::list_all_edges` actually returns the relationships used for destination-visibility blocking (if not, surface them).
- Expose move on each domain's transports: CLI `move` subcommand (dry-run + resume/rollback), MCP `move_preflight/move_apply/move_resume/move_rollback` tools, and the HTTP move endpoint, mirroring the ticket surface.
- Consider extracting the shared `to_move_error`/`from_move_error` mapping and the no-op board/lease hooks into kernel-provided defaults so domain adapters shrink to the methods they actually need.

## Non-goals

- Changing the kernel's atomicity/journal model or the fail-closed board policy.
- Cross-store transactional move.

## Acceptance criteria

- [ ] At least `spec-api` and `rule-api` expose move over CLI, MCP, and HTTP using the shared kernel, with parity to the ticket surface.
- [ ] The `spec-api` adapter relinks spec hierarchy + code refs + fulfillment and rebuilds the slug index on move (or documents, with a test, why a given reference class needs no rewrite).
- [ ] Destination-visibility blocking is proven for each new domain (a move that would strand a reference is rejected).
- [ ] `MoveDomain` boilerplate is reduced via default trait methods where the kernel can supply them.
- [ ] No move logic duplicated across domain crates.

## Relationship / traceability

- Depends on `0a510279` (generic move kernel) — the enabling work.
- Sibling surfaces to mirror: ticket move CLI `53176121`, MCP `84d19fab`, HTTP `373a3317`.
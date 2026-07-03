# Adopt the generic move kernel across domains and expose move on all transports

## Goal

Now that the domain-neutral move kernel (`0a510279`, `memory_api::storage::move_kernel`) has landed and is proven by the `ticket-api` adapter plus a `spec-api` demonstration adapter, make cross-workspace move a first-class, surfaced capability for supported domain stores and their implemented transports.

## Problem / current state

- `ticket-api` and `spec-api` expose move over their CLI, MCP, and HTTP surfaces.
- `rule-api`, `audit-api`, and `session-api` expose move over their CLI and MCP surfaces.
- The repository currently has HTTP crates for `ticket-http`, `spec-http`, and `doc-http` only. There is no `rule-http`, `audit-http`, or `session-http` transport to wire, so this ticket does not require creating new HTTP crates as part of move adoption.
- `test-api`, `log-api`, and `doc-api` do not yet have move adoption scoped by this ticket.

## Scope

- Implement `MoveDomain` for each scoped domain that should support move (`spec-api`, `rule-api`, `audit-api`, and `session-api`), reusing the kernel; no duplicated move logic.
- Harden the `spec-api` adapter to production quality: enumerate ALL spec references (hierarchy + code refs + fulfillment), relink them on move, and rebuild the slug index post-move; confirm `EntityStore::list_all_edges` actually returns the relationships used for destination-visibility blocking (if not, surface them).
- Expose move on each scoped domain's implemented transports: CLI `move` subcommand (dry-run + resume/rollback), MCP `move_preflight/move_apply/move_resume/move_rollback` tools, and HTTP move endpoints only for domains that already have an HTTP crate.
- Consider extracting the shared `to_move_error`/`from_move_error` mapping and the no-op board/lease hooks into kernel-provided defaults so domain adapters shrink to the methods they actually need.

## Non-goals

- Changing the kernel's atomicity/journal model or the fail-closed board policy.
- Cross-store transactional move.
- Creating new domain HTTP crates such as `rule-http`, `audit-http`, or `session-http`.

## Acceptance criteria

- [ ] At least `spec-api` and `rule-api` expose move over CLI and MCP using the shared kernel, with parity to the ticket surface.
- [ ] Domains with existing HTTP transports expose move over HTTP using the shared kernel; currently this means `ticket-http` and `spec-http`.
- [ ] The `spec-api` adapter relinks spec hierarchy + code refs + fulfillment and rebuilds the slug index on move (or documents, with a test, why a given reference class needs no rewrite).
- [ ] Destination-visibility blocking is proven for each new domain (a move that would strand a reference is rejected).
- [ ] `MoveDomain` boilerplate is reduced via default trait methods where the kernel can supply them.
- [ ] No move logic duplicated across domain crates.

## Relationship / traceability

- Depends on `0a510279` (generic move kernel) — the enabling work.
- Sibling surfaces to mirror: ticket move CLI `53176121`, MCP `84d19fab`, HTTP `373a3317`.
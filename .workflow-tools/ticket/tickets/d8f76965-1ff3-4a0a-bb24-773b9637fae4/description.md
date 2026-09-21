# session-api: cascade context-gathering search

Given a `ticket_id` at `session_init`, proactively gather selective context from rules, specs, and tickets across stores by following **hard ID links only** (D5), resolved as URNs.

## Scope
- Follow hard links: ticket→spec (cross-entity edges), ticket→ticket (`depends_on`), rule→entity (scoped attachment). Emit auto-pin suggestions with `reason`.
- Resolve each related entity to a URN via the cross-store resolver.
- Return a suggestion set `init_context` persists into `pinned_entities` (each pin emits a usage event).
- Degrade gracefully: missing store / unresolved link → per-suggestion diagnostic, not a hard failure.
- No semantic auto-pinning of vague matches.

## Depends on (cross-store references — must be robust first)
- graph-edged: [82d6ada4 URN resolver], [6bd67a7a multi-store discovery], [b03be2d5 cross-entity edges spec↔ticket], [f00291a3 ticket↔spec integration].
- Builds on the runtime session-context model (412964a3).

## Refinement note (REQUIRED before implementation)
This ticket is now ready for design refinement, but not for implementation. The remaining gaps are narrower and concrete:
- ticket→spec links are free-text in spec Traceability today (not structured edges);
- rules attach by `path_scope`/`repo_scope`, not by an entity id, so a rule→entity "hard link" and a rule URN shape are undefined.

**Refine this ticket now against the delivered URN/discovery model and the planned hard-link tickets** (b03be2d5 + f00291a3). Replace the provisional link-following rules above with the exact edge kinds and URN forms the resolver is expected to expose, then finalize the acceptance criteria in the spec. Do not start cascade implementation until b03be2d5 and f00291a3 are done and the rule-entry link shape is explicit.

## Spec
`memory-api/session-api/cascade-context-gathering` (fda5c915).
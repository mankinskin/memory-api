Plan a feedback store that ingests human and privileged-agent feedback events, normalizes metadata, and supports deep search and reconciliation at scale.

This program is the SINGLE owner of entity feedback AND entity usage signals. There is no separate/parallel "generic usage & feedback" model — the bootstrap-facing curation surface is delivered as part of this program (the prior parallel ticket f8b447b7 has been cancelled and folded in here).

## Acceptance criteria
- schema/indexing and query model are specified
- ingestion, search, and reconciliation work are split into executable tickets
- quality and abuse boundaries are explicit in planning

## Downstream consumer requirements — session bootstrapping (effba966)
The session-bootstrap epic depends on the FULL feedback-api program. feedback-api must be shaped so these consumer needs are first-class, not bolted on:

1. **URN-addressed entity references.** Feedback and usage events are keyed by `ce://<workspace>/<store>/<entity>` URNs so specs, rules, and (later) tickets share one addressing scheme. Coordinate with the URN resolver (default-store tickets 82d6ada4 / 6bd67a7a).
2. **Usage counting.** Record a usage event per entity URN, emitted whenever a session pins an entity. Expose aggregate count + last-used per entity. This is the "usage frequency" signal session pinning must feed.
3. **Entity feedback ratings.** `helpful` / `mixed` / `not-helpful` + optional note, optional `session_id` / `agent_or_user_id`. Agents rate pinned entities before responding.
4. **Entity coverage now vs later.** Wire spec and rule entities now (rule-entry feedback already exists; spec feedback per memory-api-store ticket 29bf9628 should be subsumed here, not built separately). Leave a compile-checked extension point for ticket entities.
5. **Curation query surfaces.** Query entities by usage frequency (hot/cold), and query low-rated / unresolved-note entities, so obsolete or low-value rules/specs surface for cleanup.

These requirements MUST be reflected in the program's child tickets (notably ingestion 9c95c1e4) before the session-bootstrap implementation starts.

## Spec
`memory-api/curation/entity-usage-and-feedback` (71b81a55) now describes the bootstrap-facing curation contract owned by this program.
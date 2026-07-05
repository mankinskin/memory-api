Define feedback event ingestion for human and privileged-agent authors, normalize metadata, and establish retention/privacy boundaries.

## Scope extension — bootstrap-facing curation events (folded from cancelled f8b447b7)
Ingestion must accept and persist, keyed by `ce://<workspace>/<store>/<entity>` URN:

- **Usage events** — one per entity pin emitted by session bootstrapping; aggregate to count + last-used.
- **Rating events** — `helpful` / `mixed` / `not-helpful` + optional note, optional `session_id` / `agent_or_user_id`.

Wire spec and rule entities now (subsume direct spec feedback ticket 29bf9628 in the memory-api store); leave a compile-checked extension point for ticket entities. Expose query surfaces: entities by usage frequency, and low-rated / unresolved-note entities.

These are hard requirements for the session-bootstrap consumers (epic effba966, runtime 412964a3) and must land before session-bootstrap implementation begins.

## Foundation carried forward — completed bootstrap gate (c7542933, in-review)
Ingestion builds directly on the completed core curation surface delivered by the bootstrap gate `c7542933` (edge: `9c95c1e4 depends_on c7542933`):

- **Implementation delivered (gate):** persisted URN feedback core store in `rule-api` with usage/rating NDJSON logs, frequency / low-rated / unresolved queries, rule+spec wiring helpers, and a ticket-URN extension point.
- **Validation evidence (gate, passing):** `cargo test -p rule-api feedback::`; `cargo test -p rule-api record_feedback_appends_event_log_and_updates_summary`; `cargo test -p rule-api low_rated_rule_is_badged`.
- **Consequence:** ingestion extends this core store rather than reimplementing it; usage/rating persistence, query surfaces, and URN keying are reused.

## Ingestion-first execution order
Land the canonical URN usage + rating ingestion, metadata normalization, and retention/privacy boundaries on top of the gate store first, so downstream surfaces can rely on one authoritative store:

1. Ingestion + normalization + retention (this ticket) on top of gate `c7542933`.
2. Runtime session-context (`412964a3`) consumes the canonical URN usage/rating store.
3. session-cli / session-mcp pin/unpin (`6b2dc497`) emit usage events into the same store.

Dependency drift note: the cancelled `f8b447b7` lineage has been removed from `412964a3` and `6b2dc497`; both now depend on this ingestion ticket so live bootstrap gating matches the full-feedback plan.
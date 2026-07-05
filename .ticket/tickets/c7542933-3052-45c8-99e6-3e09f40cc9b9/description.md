# feedback-api CORE curation surface (bootstrap gate)

This is the **minimal, self-contained core** of the feedback-api program that session bootstrapping gates on — NOT the full program. It exists so the session-bootstrap epic can depend on a small, shippable milestone instead of the heavyweight feedback-api program (at-scale search/clustering, SLOs, abuse governance, retention/redaction).

## Scope (core only)
- A minimal record + query store for two event kinds, keyed by URN `ce://<workspace>/<store>/<entity>`:
  - **usage events** — one per session pin; aggregate to count + last-used.
  - **rating events** — `helpful` / `mixed` / `not-helpful` + optional note, optional `session_id` / `agent_or_user_id`.
- Wire **spec** and **rule** entities now; compile-checked extension point for **ticket** entities.
- Query surfaces: entities by usage frequency; low-rated / unresolved-note entities.

## Explicitly NOT in core (deferred to the full program b1e9e744)
- At-scale search, clustering, reconciliation.
- Search-latency / index-growth SLOs.
- Abuse-boundary governance and privileged-author policy.
- Retention, redaction, privacy-incident controls.

## Relationship
- Parent program: `b1e9e744` (full feedback-api) — the program `depends_on` this core child; the broader slices build on top.
- Implements the contract in spec `memory-api/curation/entity-usage-and-feedback` (71b81a55).
- **Gates:** session-bootstrap epic `effba966` and runtime `412964a3` depend on THIS ticket (the core), not the full program.

## Done when
Usage events + ratings record and query generically across spec and rule URNs with unit tests; ticket extension point compiles; query-by-frequency and low-rated queries return correct results. No dependency on the heavyweight program slices.

## Placement note
memory-api-domain ticket currently created in the context-engine store so it can be graph-edged to the session-bootstrap cluster; it is a migration candidate for relocation into the memory-api store (see cleanup 7599ed31).
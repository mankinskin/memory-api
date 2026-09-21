# Generic entity usage counting & feedback ratings (memory-api)

Prerequisite for the [session-bootstrap] epic. Provides the generic, entity-type-agnostic curation primitive that session bootstrapping feeds.

## Why
Session pinning must record **usage frequency**, and agents must be able to **rate** pinned entities (`helpful`/`mixed`/`not-helpful`) before responding, so we can curate useful rules/specs and detect obsolete low-value entries. No usage counter exists today, and feedback is fragmented (rule feedback done; spec feedback planned separately). This ticket unifies them behind one generic, URN-addressed model.

## Scope
- Generic `EntityRef` (URN `ce://<workspace>/<store>/<entity>`) usage + feedback model in memory-api, independent of concrete store type.
- Usage counting: record a usage event per entity URN (emitted on each session pin); expose aggregate count + last-used.
- Feedback ratings: `helpful`/`mixed`/`not-helpful` + optional note, optional `session_id`/`agent_or_user_id`.
- Query surface: entities by usage frequency; entities low-rated / with unresolved notes.
- Wire **spec** and **rule** entities now; leave a compile-checked extension point for **ticket** entities (no ticket wiring required).

## Out of scope
- Heavyweight `feedback-api` ingestion/search/SLO/governance program (separate, future).
- Curation UI.

## Relationship to existing planning
- Reuses rule-entry feedback shape (done).
- Aligns with / can subsume direct spec feedback (memory-api store ticket 29bf9628).
- Informs but does not block on the broader feedback-api program (b1e9e744).

## Spec
`memory-api/curation/entity-usage-and-feedback` (71b81a55).

## Done when
Usage events + ratings record and query generically across spec and rule URNs with tests; ticket extension point compiles.
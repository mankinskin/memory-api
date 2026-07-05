Define feedback event ingestion for human and privileged-agent authors, normalize metadata, and establish retention/privacy boundaries.

## Scope extension — bootstrap-facing curation events (folded from cancelled f8b447b7)
Ingestion must accept and persist, keyed by `ce://<workspace>/<store>/<entity>` URN:

- **Usage events** — one per entity pin emitted by session bootstrapping; aggregate to count + last-used.
- **Rating events** — `helpful` / `mixed` / `not-helpful` + optional note, optional `session_id` / `agent_or_user_id`.

Wire spec and rule entities now (subsume direct spec feedback ticket 29bf9628 in the memory-api store); leave a compile-checked extension point for ticket entities. Expose query surfaces: entities by usage frequency, and low-rated / unresolved-note entities.

These are hard requirements for the session-bootstrap consumers (epic effba966, runtime 412964a3) and must land before session-bootstrap implementation begins.
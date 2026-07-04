# session-api: runtime cognitive-workspace model

Extend `session-api` (`memory-api/crates/session-api`) from a write/archive store into a read/runtime store.

## Decisions baked in
- **D1/D9 resume:** `init_context(session_id)` is load-or-create + idempotent; later turns resume and keep mutating. Creates the session dir on init (**D4**).
- **D2 URNs:** pinned entities are stored as cross-store URNs `ce://<ws>/<store>/<id>`.
- **D4 persistence:** flush `session_context.json` per mutation.
- **D6 headers-only:** `render_view` returns short headers (urn, type, title|slug, relation, reason), never full bodies.
- **D8 no mode:** no `current_mode`; general chat = empty pins.
- **D9 usage:** each `pin` emits one usage event into the feedback-api CORE curation model.

## Scope
- Add `session_context.json` alongside `session.json`/`transcript.json` without breaking the capture/archive path.
- Core ops: `init_context`, `pin`, `unpin`, `read_context`, `render_view`.
- Focused unit tests incl. a regression proving the capture path output is byte-identical.

## Depends on
- Design ticket (schema/ADRs frozen).
- [82d6ada4 URN cross-store resolver] — context stores entity refs as URNs.
- [c7542933 feedback-api CORE curation surface] — pin emits usage events.

## Spec
`memory-api/session-api/runtime-session-context` (709f067a).
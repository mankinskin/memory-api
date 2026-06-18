# Goal
Persist `session-api` capture requests into a deterministic filesystem layout that can become the first memory-api-backed session store.

# Scope
- keep the existing `SessionCaptureRequest -> SessionRecord -> SessionStorePlan` flow
- add the first write path that materializes the planned session directory and JSON files
- serialize a session manifest and transcript payload in a stable, readable format
- return explicit IO and serialization errors from the write path
- validate persistence with focused unit tests against a temporary directory

# Non-goals
- watcher-based ingestion
- CLI, MCP, or HTTP session capture commands
- query or search APIs over persisted sessions
- schema migration or retention logic beyond the planned file layout

# Acceptance Criteria
1. `session-api` can persist a capture request into the planned filesystem layout.
2. The write path creates the session directory and writes stable JSON files for metadata and transcript content.
3. Error handling distinguishes serialization and filesystem failures.
4. Focused unit tests prove file creation and persisted content shape.

# Traceability
- Ticket: [c8f79641 session persistence](C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-api/.ticket/tickets/c8f79641-6f99-4401-9b08-ad960a8d785c/ticket.toml)

# Implemented Slice
- Added `PersistedSessionManifest` and `PersistedSessionTranscript` as the first stable on-disk JSON shapes.
- Added `SessionStorePlan::persist()` to create the session directory and write `session.json` plus `transcript.json`.
- Added `SessionStoreConfig::persist_capture()` to combine planning with the real write path.
- Extended `SessionError` with explicit serialization and filesystem failure variants.
- Added focused persistence tests that verify created files and overwrite behavior for repeat writes to the same session id.

# Validation
- ValidationSpec: focused `session-api` persistence tests for filesystem creation and persisted JSON shape.
- ValidationExecution: passed `cargo test -p session-api`.

# Evidence Mapping
- DocEvidenceRecord candidates: `crates/session-api/Cargo.toml`, `crates/session-api/src/error.rs`, `crates/session-api/src/lib.rs`, and `crates/session-api/src/store.rs`.
- ValidationLogCapture / ValidationLogRetrieval: `cargo test -p session-api` output captured in the current terminal session.

# Remaining Work
- Add higher-level ingestion surfaces that emit `SessionCaptureRequest` payloads.
- Decide whether transcript writes should become append-only or remain latest-state overwrites.
- Add query/search APIs once the stored file layout is consumed by other memory-api surfaces.
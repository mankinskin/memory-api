<!-- aligned-structure:v1 -->

# Summary

Extend `session-api` so repeated Copilot hook captures preserve transcript history as an append-only log and expose a first read/query API over the persisted store.

## Behavior Story

Extend `session-api` so repeated Copilot hook captures preserve transcript history as an append-only log and expose a first read/query API over the persisted store.

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# Goal
Extend `session-api` so repeated Copilot hook captures preserve transcript history as an append-only log and expose a first read/query API over the persisted store.

# Scope
- make repeated session captures append only at the transcript layer
- keep the existing session manifest path and session directory layout
- add read APIs that reconstruct stored sessions from manifest plus transcript files
- add a small query surface for listing and filtering stored sessions within one workspace slug
- add the first hook-facing helper that persists a Copilot hook payload directly through the store
- cover the behavior with focused unit tests in `session-api`

# Non-goals
- a CLI, MCP, or HTTP endpoint for session ingestion
- a database index for sessions
- pagination or ranking beyond a small in-memory query filter
- hook installation scripts or editor integration

# Acceptance Criteria
1. Persisting a later capture for the same session never removes or replaces earlier transcript turns; only the new suffix is added.
2. `session-api` can read a persisted session back into a `SessionRecord`.
3. `session-api` can query stored sessions by simple metadata and transcript text filters within one workspace.
4. `session-api` exposes a hook-facing helper that takes a `CopilotHookPayload` and persists it through the store.
5. Focused unit tests cover append-only persistence, session reads, hook capture, and query behavior.

# Traceability
- Ticket: [959c94bd session hook ingestion and read/query](C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-api/.ticket/tickets/959c94bd-4a42-47d6-bee4-a12332a23b52/ticket.toml)
- Prior slice: persistence writer spec `823b22cf-c0dc-46c6-a03d-00cdd3c4c83a`

# Implemented Slice
- Added append-only transcript merging so repeated captures extend the stored transcript instead of replacing prior turns.
- Added read helpers that reconstruct a `SessionRecord` from persisted `session.json` and `transcript.json`.
- Added a `SessionQuery` filter surface and query API over the workspace-scoped session store.
- Added a hook-facing `capture_copilot_hook` helper that accepts a `CopilotHookPayload` and persists it through the store.
- Added explicit read-side error variants for missing, deserialization, and transcript-rewrite-conflict cases.

# Validation
- ValidationSpec: focused `session-api` store and hook tests for append-only transcript persistence, session reads, query filtering, and hook capture.
- ValidationExecution: passed `cargo test -p session-api`.

# Evidence Mapping
- DocEvidenceRecord candidates: `crates/session-api/src/error.rs`, `crates/session-api/src/lib.rs`, and `crates/session-api/src/store.rs`.
- ValidationLogCapture / ValidationLogRetrieval: `cargo test -p session-api` output captured in the current terminal session.

# Remaining Work
- Expose the hook capture path through a CLI, MCP, HTTP, or editor-facing surface.
- Add indexing or richer query semantics if session volume grows beyond in-memory scanning.
- Decide whether session manifests should become atomic with transcript writes instead of best-effort paired filesystem updates.

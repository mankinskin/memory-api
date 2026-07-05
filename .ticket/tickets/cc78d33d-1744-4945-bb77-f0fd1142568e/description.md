# [memory-matrix] Capture subprocess failure bundles for transport test triage

## Goal

Make matrix harness failures for subprocess-driven transport cells actionable on first failure, without ad hoc stderr plumbing.

## Problem

When transport/subprocess tests fail (for example MCP stdio sentinel/protocol failures), the first error is often opaque. Developers recover detail by custom stderr plumbing and manual protocol-context reconstruction. This slows triage and review and breaks repeatability.

## Scope

For subprocess-driven matrix cells (CLI/MCP/HTTP child processes as applicable), capture and persist a diagnostics failure bundle with:

- command/executable and args
- environment selectors used by the harness (explicitly whitelisted keys only)
- cwd/workspace selector
- child stdout tail and child stderr tail (bounded, redacted)
- structured error class (spawn failure, handshake timeout, protocol sentinel mismatch, unexpected EOF, non-zero exit, parse/decode error, assertion mismatch)
- correlation metadata: run_id, cell_id, transport, operation, request/tool id where available
- linkage metadata: log session id(s), journal id if applicable, test execution id

Integrate with existing runtime log session and transport correlation work so bundles are indexed/searchable rather than terminal-only.

## Acceptance criteria

- When a subprocess-driven matrix cell fails, the returned failure artifact includes all bundle fields above.
- Failure output persists to a queryable store location and is linked to a test execution id and log session id.
- Harness output includes a stable correlation/run id that can be used to retrieve the persisted artifact.
- At least one MCP stdio sentinel failure path and one non-zero-exit path are covered by tests.
- Bounded stdout/stderr tails are captured with redaction policy applied.
- Diagnostic bundle retrieval is documented in the validation matrix.

## Validation

- Reproduce with:
  - `cargo test -p memory-matrix --test matrix ticket_get_mcp_cell_is_wired_and_passes -- --nocapture`
  - or `cargo test -p memory-matrix --test matrix every_cell_records_an_execution_with_duration -- --nocapture`
- Confirm failure evidence is retrievable by correlation id and includes linked log session + test execution metadata.

Improvement-pass deterministic checks:

- `cargo test -p memory-matrix subprocess_probe_persists_full_failure_bundle_fields -- --nocapture`
- `cargo test -p memory-matrix subprocess_spawn_probe_reports_spawn_failure_bundle -- --nocapture`
- `cargo test -p memory-matrix mcp::tests:: -- --nocapture`

These checks now cover additional deterministic angles:

- sentinel mismatch validation (`protocol_sentinel_mismatch`) via unit-level id guard assertions
- parse/decode failure handling (`parse_decode_error`) for invalid MCP tool text payloads
- spawn/read failure classification boundaries (`spawn_failure`, `non_zero_exit`, `unexpected_eof`, `io_read_failure`)
- env-selector redaction by whitelist and bounded output-tail assertions
- execution correlation tightening: persisted execution provenance `run_id` equals bundle `correlation.run_id`

## Dependencies

- [3041d7e3](.ticket/tickets/3041d7e3-2b34-4597-b354-e0aa6ffb0459/ticket.toml) transport correlation fields
- [d3349747](.ticket/tickets/d3349747-b2f2-4dd4-b73c-dc016fec80d6/ticket.toml) runtime log session metadata
- [bce26d30](.ticket/tickets/bce26d30-0a79-40b4-812a-c14b4a246de5/ticket.toml) validation/documentation matrix

Blocker coordination reviewed in this pass:

- [60a2a388](memory-viewers/.ticket/tickets/60a2a388-c8b6-4e25-a80a-0ba686f11bf9/ticket.toml) `[LOG-1b] doc-viewer + spec-viewer: wire init_tracing_full with file logging` (state: `new`)
- [12197242](memory-api/.ticket/tickets/12197242-b7b4-4212-83a8-4b0b65a4bd7b/ticket.toml) `[LOG-2a] tracing field-name normalization for log-viewer compatibility` (state: `new`)

No new dependency edge was added in this pass because existing `depends_on` coverage already captures runtime-correlation prerequisites (`3041d7e3`, `d3349747`) and the reviewed blockers remain cross-workspace coordination items.

## Non-goals

- Replacing transport protocol behavior or matrix execution semantics.
- Capturing unbounded process output.
- Logging full unredacted environment.

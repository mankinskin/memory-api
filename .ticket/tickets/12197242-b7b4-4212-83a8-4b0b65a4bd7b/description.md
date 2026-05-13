# Audit and normalise context-* tracing field names

## Problem

The `crates/context-{insert,read,search,trace}` crates emit `tracing` spans and events but field names, targets, and event shapes are not uniform. The log-viewer parser (`crates/context-api/src/log_parser.rs`) expects a specific JSON schema:

```json
{
  "timestamp": "...",
  "level": "INFO",
  "fields": { "message": "...", ... },
  "target": "...",
  "span": "...",
  "filename": "...",
  "line_number": 123
}
```

Without consistent field naming, JQ queries against context-* log files break or miss entries.

## Scope

1. **Audit** — enumerate all `tracing::debug!` / `info!` / `warn!` / `error!` / `span!` calls across:
   - `crates/context-trace/src/` (especially `trace/traceable/`, `graph/visualization.rs`)
   - `crates/context-insert/src/` (especially `visualization.rs`, `join/`)
   - `crates/context-read/src/`
   - `crates/context-search/src/`
2. **Define a field-name convention** — document in `CHEAT_SHEET.md` or a new `docs/tracing-schema.md`:
   - Mandatory fields: `target`, event type naming convention
   - Recommended field names for common concepts: `workspace`, `root_id`, `depth`, `elapsed_ms`, `nodes`, `edges`, `op`
3. **Normalise** — rename fields where they diverge from the convention (mechanical find/replace; no behaviour change).
4. **Verify parser compatibility** — confirm that after normalisation, `crates/context-api/src/log_parser.rs` correctly parses a representative `target/test-logs/*.log` file.

## Acceptance Criteria

- A field naming convention is documented.
- All context-* tracing events use snake_case field names and consistent targets.
- `mcp_log-viewer-mc_query_logs` with `select(.target | startswith("context_"))` returns entries for each of the four crates when a test producing logs is run.
- No test regressions.

## Files

- `crates/context-trace/src/**/*.rs`
- `crates/context-insert/src/**/*.rs`
- `crates/context-read/src/**/*.rs`
- `crates/context-search/src/**/*.rs`
- `crates/context-api/src/log_parser.rs` (read; may not need changes)
- `CHEAT_SHEET.md` or new `docs/tracing-schema.md`

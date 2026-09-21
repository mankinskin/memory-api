# Epic: Unified Logging Infrastructure

Provide every viewer-api tool and context-* crate with consistent, queryable, structured logging.

This epic also owns the memory-system observability track for `memory-api` domain crates and CLI/MCP/HTTP transports. The detailed contract lives in spec `aa769a27` (`memory-api/observability/runtime-logging`) and tracker `73b2cd22`.

## Motivation

Currently:
- `ticket-viewer`, `doc-viewer`, `spec-viewer` use `init_tracing()` (console-only; logs are lost when detached)
- `log-viewer` already has file-logging wired but other tools do not
- `context-*` crates emit `tracing` spans/events that only appear in `target/test-logs/` during tests
- The log-viewer Preact frontend has no Dioxus port (inconsistent with ticket-viewer/spec-viewer)
- No Mermaid diagram or CLI table rendering exists for log data
- Browser (WASM) logs are not correlated with server logs
- `memory-api` domain crates and transports initialize tracing unevenly and do not yet register runtime log sessions in `log-api`

## Goals

1. **File sink for all tools** — every `*-viewer` and `*-http` server writes JSONL logs to `target/logs/` at startup via `init_tracing_full`
2. **context-* structured log capture** — ensure crates/context-{insert,read,search,trace} emit well-structured spans that land in the same log files
3. **Log schema search** — structured field search across log files (extend log-viewer MCP + HTTP API)
4. **Log text search** — full-text/regex search across log files
5. **Log-to-Mermaid** — convert a log session (spans + events) to a sequence diagram
6. **Log-to-CLI table** — render a filtered log view as an ASCII/Markdown table for terminal use
7. **Log-viewer Dioxus frontend** — port the Preact log-viewer SPA to Rust/Dioxus (aligns with the Dioxus Viewer Platform epic)
8. **Memory-system runtime diagnostics** — shared tracing initialization and log-api runtime session metadata for memory-domain crates and CLI/MCP/HTTP transports

## Track Breakdown

### Track 1 — File Sink Integration (3 tickets)
Wire `init_tracing_full` with `with_file_logging` into every tool that currently uses `init_tracing`.

- `[LOG-1a]` ticket-viewer: wire file-logging via `init_tracing_full`
- `[LOG-1b]` doc-viewer + spec-viewer: wire file-logging via `init_tracing_full`
- `[LOG-1c]` viewer-ctl: add `--log-dir` / `--log-level` start flags that set env vars for the launched server

### Track 2 — context-* Structured Log Schema (2 tickets)
Ensure all crates/context-{insert,read,search,trace} spans are schema-consistent and compatible with the log-viewer parser.

- `[LOG-2a]` Audit and normalise context-* tracing field names
- `[LOG-2b]` Add `context-trace` JSON format compatibility test against log-viewer parser

### Track 3 — Log Schema & Text Search (2 tickets)
Extend the log-viewer MCP + HTTP server with richer search capabilities.

- `[LOG-3a]` Schema-field search: filter log entries by any structured field (extend `query_logs`)
- `[LOG-3b]` Full-text search: add `search_text` MCP tool + HTTP endpoint with optional regex

### Track 4 — Rendering (2 tickets)
New output formats for log data.

- `[LOG-4a]` Log-to-Mermaid: convert a filtered log session to a `sequenceDiagram`
- `[LOG-4b]` Log-to-table: render a filtered log view as an ASCII/Markdown table (`log-to-table` CLI subcommand or MCP tool)

### Track 5 — Dioxus Frontend (3 tickets)
Port the Preact log-viewer SPA to Dioxus WASM.

- `[LOG-5a]` Scaffold log-viewer-dioxus crate + trunk build
- `[LOG-5b]` Port log browser UI (file tree, entry list, search bar, stats)
- `[LOG-5c]` Add live-tail view (SSE-backed, reuse viewer-api SSE infrastructure)

### Track 6 — memory-api Runtime Diagnostics (1 tracker)
Define and implement the DRY tracing/log-api architecture for memory-domain crates and transports.

- `[memory-api] Shared tracing and log-api runtime diagnostics` (`73b2cd22`) — shared initialization, runtime log session metadata, domain instrumentation map, transport correlation, live indexing/search, and documentation

## Dependency Order

```
Track 1 (file sinks) → Track 2 (schema) → Track 3 (search)
Track 3 → Track 4 (rendering)
Track 1 → Track 5 (Dioxus frontend needs live log files)
Track 1/2 → Track 6 (reuse existing file-sink and schema work without duplicating tracing infrastructure)
```

## Done Condition

All tracks complete. Every tool writes structured JSONL logs to `target/logs/` at startup. The log-viewer Dioxus frontend replaces the Preact frontend. MCP and HTTP APIs support schema-field search, text search, Mermaid export, and table export.

## Related Epics

- `35a6d14b` — Epic: Dioxus Viewer Platform (Track 5 aligns with that epic's architecture)
- `b480632a` — viewer-api-dioxus: structured tracing for WASM frontend (Track 1 dependency)
- `8f349d96` — viewer-api-dioxus: ship WASM tracing logs to server file sink (Track 1 dependency)
- `73b2cd22` — memory-api shared tracing and log-api runtime diagnostics (Track 6)

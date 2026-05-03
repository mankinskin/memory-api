---
description: "Use when creating or editing tests, benchmarks, or stress tests. Covers tracing setup, test-log debugging, Criterion benchmarks, and HTTP-level validation."
applyTo: "**/tests/**,**/*test*,**/benches/**"
---

# Testing Guidance

## Quick Reference — Common Commands

```bash
# Run a single test by name (fastest, use first)
cargo test -p <crate> <test_name> -- --nocapture

# Run all tests in a crate
cargo test -p ticket-api
cargo test -p ticket-http

# Run Criterion benchmarks (ticket-api graph pipeline)
cargo bench --bench graph_ops -p ticket-api

# HTTP-level stress test (requires running ticket-viewer server)
python tools/http/stress_graph.py          # concurrency sweep 2–32
python tools/http/bench2.py               # verbose per-request phase timing

# Full workspace test (slow — only after local crate tests pass)
cargo test
```

## Criterion Benchmarks

The BFS graph query pipeline is benchmarked in `crates/ticket-api/benches/graph_ops.rs`.
Run with: `cargo bench --bench graph_ops -p ticket-api`

| Benchmark | What it measures |
|---|---|
| `phase1_list_all_edges` | ReDB edge table scan (~630 edges) |
| `phase2_bfs_in_memory` | Pure in-memory BFS, no DB |
| `phase3_get_indexed_many` | Batch metadata fetch (1 ReDB transaction, 39 nodes) |
| `phase3_get_indexed_one_by_one` | Per-node fetch baseline (39 separate transactions) |
| `pipeline_full` | All 3 phases end-to-end |
| `pipeline_concurrent/{2,4,8,16,32}` | N threads barrier-synchronized |

The fixture builds 360 tickets + ~630 edges once per process (via `OnceLock`).

When adding a new storage-layer optimization, add a matching Criterion benchmark that shows the before/after comparison.

## HTTP-Level Stress Testing

`tools/http/stress_graph.py` — concurrency sweep (phases 1–3, with optional soak):

```bash
python tools/http/stress_graph.py                    # default workspace, depth=4
python tools/http/stress_graph.py --base-url http://127.0.0.1:3002 --depth 4
```

`tools/http/bench2.py` — verbose single-run timing including server-side phase breakdown from the `stats` field in the response body.

**Windows note**: always use `127.0.0.1` (not `localhost`) in `--base-url`. Windows resolves `localhost` to IPv6 (`::1`) first; the server only binds IPv4, causing ~2s connection timeout per request before fallback.

## Deploying ticket-viewer for HTTP Testing

```bash
# Build the binary (must build the viewer, not just the library)
cargo build -p ticket-viewer --release

# Deploy and restart
viewer-ctl stop ticket-viewer
viewer-ctl install ticket-viewer
viewer-ctl start ticket-viewer
```

The binary is `~/.cargo/bin/ticket-viewer.exe`. Building only `-p ticket-http` produces
the library but not the binary; the server will be stale until the viewer crate is rebuilt.

## Tracing Setup

For tracing-based tests, initialize tracing with graph context:

```rust
let _tracing = init_test_tracing!(&graph);
```

This improves readability of tokens and graph state in logs.

## Debug Workflow

When a test fails:
1. Run targeted tests first.
2. Inspect `target/test-logs/` for full trace output.
3. Use log-viewer MCP tools (`query_logs`, `search_all_logs`) with jq filters instead of parsing logs manually.

## Test Execution Strategy

- Start with nearest unit/integration tests.
- Expand to crate-level runs once local failures are resolved.
- Avoid unrelated full-workspace test runs unless required.

For frontend-impacting changes:

- Run lint and typecheck in each affected frontend package.
- Run nearest unit/component tests for changed UI code.
- Run at least one browser-based end-to-end path that covers changed UX behavior.

For viewer/API integration changes:

- Add or run assertions that verify the viewer contract with context-api or ticket-api for changed endpoints.
- For filesystem-backed behaviors, include path-handling and access-boundary assertions.

For performance-sensitive paths (storage, BFS, graph queries):

- Add or run a Criterion benchmark in `crates/<crate>/benches/`.
- Confirm `phase3_get_indexed_many` is used instead of repeated `get_indexed()` calls.

For regression fixes:

- Prefer a failing reproducer assertion before or with the fix.
- Keep regression coverage focused on the reported failure mode.

## Assertions

- Prefer assertions that check behavior, not incidental implementation details.
- Keep regression tests focused on the bug or contract being changed.

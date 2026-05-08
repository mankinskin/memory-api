<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=cbdb1676-ede7-41c8-9043-7813ef91de9c slug=shared/instructions/tests/tests-instructions/l1 -->
---
description: "Use when creating or editing tests, benchmarks, or stress tests. Covers tracing setup, test-log debugging, Criterion benchmarks, and HTTP-level validation."
applyTo: "**/tests/**,**/*test*,**/benches/**"
---

<!-- rule-api:entry id=dbb50505-dc78-4789-abae-bd9219d3803e slug=shared/instructions/tests/run-a-single-test-by-name-fastest-use-first/l8 -->
## Quick Reference — Common Commands

```bash
# Run a single test by name (fastest, use first)
cargo test -p <crate> <test_name> -- --nocapture

<!-- rule-api:entry id=6ae213ee-600e-4681-a6ce-c963e25e5fe7 slug=shared/instructions/tests/run-all-tests-in-a-crate/l14 -->
# Run all tests in a crate
cargo test -p ticket-api
cargo test -p ticket-http

<!-- rule-api:entry id=9914fe82-63e0-4f5e-b496-e2feec24224b slug=shared/instructions/tests/run-criterion-benchmarks-ticket-api-graph-pipeline/l18 -->
# Run Criterion benchmarks (ticket-api graph pipeline)
cargo bench --bench graph_ops -p ticket-api

<!-- rule-api:entry id=6caacef6-4541-4ae7-95bc-6737f434fc8a slug=shared/instructions/tests/http-level-stress-test-requires-running-ticket-viewer-server/l21 -->
# HTTP-level stress test (requires running ticket-viewer server)
python tools/http/stress_graph.py          # concurrency sweep 2–32
python tools/http/bench2.py               # verbose per-request phase timing

<!-- rule-api:entry id=045169a8-9c10-47ad-b60f-61eab84ece55 slug=shared/instructions/tests/full-workspace-test-slow-only-after-local-crate-tests-pass/l25 -->
# Full workspace test (slow — only after local crate tests pass)
cargo test
```

<!-- rule-api:entry id=f6dfc84a-a9a4-43d5-a896-317972ca960c slug=shared/instructions/tests/full-workspace-test-slow-only-after-local-crate-tests-pass/criterion-benchmarks/l29 -->
## Criterion Benchmarks

The BFS graph query pipeline is benchmarked in `crates/ticket-api/benches/graph_ops.rs`.
Run with: `cargo bench --bench graph_ops -p ticket-api`

<!-- rule-api:entry id=7aac128e-7f59-4a6f-a98b-10296b7045c6 slug=shared/instructions/tests/full-workspace-test-slow-only-after-local-crate-tests-pass/criterion-benchmarks/l34 -->
| Benchmark | What it measures |
|---|---|
| `phase1_list_all_edges` | ReDB edge table scan (~630 edges) |
| `phase2_bfs_in_memory` | Pure in-memory BFS, no DB |
| `phase3_get_indexed_many` | Batch metadata fetch (1 ReDB transaction, 39 nodes) |
| `phase3_get_indexed_one_by_one` | Per-node fetch baseline (39 separate transactions) |
| `pipeline_full` | All 3 phases end-to-end |
| `pipeline_concurrent/{2,4,8,16,32}` | N threads barrier-synchronized |

<!-- rule-api:entry id=4381be0c-d4b2-4e5d-a22d-6b02f62867e7 slug=shared/instructions/tests/full-workspace-test-slow-only-after-local-crate-tests-pass/criterion-benchmarks/l43 -->
The fixture builds 360 tickets + ~630 edges once per process (via `OnceLock`).

<!-- rule-api:entry id=7f20991f-20d4-4513-bf28-cbdc2f48332d slug=shared/instructions/tests/full-workspace-test-slow-only-after-local-crate-tests-pass/criterion-benchmarks/l45 -->
When adding a new storage-layer optimization, add a matching Criterion benchmark that shows the before/after comparison.

<!-- rule-api:entry id=85fcc11a-a635-43fa-bb7c-e8844cb181ee slug=shared/instructions/tests/full-workspace-test-slow-only-after-local-crate-tests-pass/http-level-stress-testing/l47 -->
## HTTP-Level Stress Testing

`tools/http/stress_graph.py` — concurrency sweep (phases 1–3, with optional soak):

<!-- rule-api:entry id=cb3cfcfe-2b33-49f7-9ace-a93d4c22d4b7 slug=shared/instructions/tests/full-workspace-test-slow-only-after-local-crate-tests-pass/http-level-stress-testing/l51 -->
```bash
python tools/http/stress_graph.py                    # default workspace, depth=4
python tools/http/stress_graph.py --base-url http://127.0.0.1:3002 --depth 4
```

<!-- rule-api:entry id=eb12a576-d5a4-48b6-a21d-34016b8dba05 slug=shared/instructions/tests/full-workspace-test-slow-only-after-local-crate-tests-pass/http-level-stress-testing/l56 -->
`tools/http/bench2.py` — verbose single-run timing including server-side phase breakdown from the `stats` field in the response body.

<!-- rule-api:entry id=a7f9e1e4-6c22-401c-a4d5-f8b2741b29f2 slug=shared/instructions/tests/full-workspace-test-slow-only-after-local-crate-tests-pass/http-level-stress-testing/l58 -->
**Windows note**: always use `127.0.0.1` (not `localhost`) in `--base-url`. Windows resolves `localhost` to IPv6 (`::1`) first; the server only binds IPv4, causing ~2s connection timeout per request before fallback.

<!-- rule-api:entry id=fcfe73f0-3dd4-48f9-ba00-d4b217f64332 slug=shared/instructions/tests/build-the-binary-must-build-the-viewer-not-just-the-library/l60 -->
## Deploying ticket-viewer for HTTP Testing

```bash
# Build the binary (must build the viewer, not just the library)
cargo build -p ticket-viewer --release

<!-- rule-api:entry id=69efa4a2-9416-4f36-b26e-f0674a6e1cbc slug=shared/instructions/tests/deploy-and-restart/l66 -->
# Deploy and restart
viewer-ctl stop ticket-viewer
viewer-ctl install ticket-viewer
viewer-ctl start ticket-viewer
```

<!-- rule-api:entry id=55cf9990-4cc5-4f59-93f9-d613da7a67a9 slug=shared/instructions/tests/deploy-and-restart/l72 -->
The binary is `~/.cargo/bin/ticket-viewer.exe`. Building only `-p ticket-http` produces
the library but not the binary; the server will be stale until the viewer crate is rebuilt.

<!-- rule-api:entry id=fded3dff-fc0e-46bf-97e5-c94443cbd056 slug=shared/instructions/tests/deploy-and-restart/tracing-setup/l75 -->
## Tracing Setup

For tracing-based tests, initialize tracing with graph context:

<!-- rule-api:entry id=1919df6b-5f85-490e-8d56-3eb6bed0bda8 slug=shared/instructions/tests/deploy-and-restart/tracing-setup/l79 -->
```rust
let _tracing = init_test_tracing!(&graph);
```

<!-- rule-api:entry id=6fd52602-620a-415b-a6c8-b31574072a61 slug=shared/instructions/tests/deploy-and-restart/tracing-setup/l83 -->
This improves readability of tokens and graph state in logs.

<!-- rule-api:entry id=f2ba2404-4258-40e9-a554-f42bde1209d6 slug=shared/instructions/tests/deploy-and-restart/debug-workflow/l85 -->
## Debug Workflow

When a test fails:
1. Run targeted tests first.
2. Inspect `target/test-logs/` for full trace output.
3. Use log-viewer MCP tools (`query_logs`, `search_all_logs`) with jq filters instead of parsing logs manually.

<!-- rule-api:entry id=05239b38-68f2-4494-b94d-2c0de1847bc6 slug=shared/instructions/tests/deploy-and-restart/test-execution-strategy/l92 -->
## Test Execution Strategy

- Start with nearest unit/integration tests.
- Expand to crate-level runs once local failures are resolved.
- Avoid unrelated full-workspace test runs unless required.

<!-- rule-api:entry id=80023125-eaee-4a32-a225-dec3c77a6ec7 slug=shared/instructions/tests/deploy-and-restart/test-execution-strategy/l98 -->
For frontend-impacting changes:

<!-- rule-api:entry id=f34fc52a-e847-4b2b-9bf6-3ab0e18727d6 slug=shared/instructions/tests/deploy-and-restart/test-execution-strategy/l100 -->
- Run lint and typecheck in each affected frontend package.
- Run nearest unit/component tests for changed UI code.
- Run at least one browser-based end-to-end path that covers changed UX behavior.

<!-- rule-api:entry id=363d2d2a-ce7a-4152-b815-68a5d4d6216d slug=shared/instructions/tests/deploy-and-restart/test-execution-strategy/l104 -->
For viewer/API integration changes:

<!-- rule-api:entry id=c7e7df9e-7134-4026-bfe4-cb693c255453 slug=shared/instructions/tests/deploy-and-restart/test-execution-strategy/l106 -->
- Add or run assertions that verify the viewer contract with context-api or ticket-api for changed endpoints.
- For filesystem-backed behaviors, include path-handling and access-boundary assertions.

<!-- rule-api:entry id=f2561069-247c-47ad-bd95-2135013ecc4a slug=shared/instructions/tests/deploy-and-restart/test-execution-strategy/l109 -->
For performance-sensitive paths (storage, BFS, graph queries):

<!-- rule-api:entry id=7dd862c3-0138-4c81-8b99-ffa02f90f0d3 slug=shared/instructions/tests/deploy-and-restart/test-execution-strategy/l111 -->
- Add or run a Criterion benchmark in `crates/<crate>/benches/`.
- Confirm `phase3_get_indexed_many` is used instead of repeated `get_indexed()` calls.

<!-- rule-api:entry id=7e2265d3-9a1f-4362-a519-fd89584d6ffb slug=shared/instructions/tests/deploy-and-restart/test-execution-strategy/l114 -->
For regression fixes:

<!-- rule-api:entry id=3e02f028-232d-4fd5-b93b-da6b9ba65803 slug=shared/instructions/tests/deploy-and-restart/test-execution-strategy/l116 -->
- Prefer a failing reproducer assertion before or with the fix.
- Keep regression coverage focused on the reported failure mode.

<!-- rule-api:entry id=57630400-2aed-4d82-a61b-db4e2a8490bc slug=shared/instructions/tests/deploy-and-restart/assertions/l119 -->
## Assertions

- Prefer assertions that check behavior, not incidental implementation details.
- Keep regression tests focused on the bug or contract being changed.

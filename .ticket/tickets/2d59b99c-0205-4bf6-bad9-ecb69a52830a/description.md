# [profiling] CLI/HTTP/MCP throughput/latency benchmarks

Child of tracker `ef3f4a91`. Add transport-level throughput/latency benchmarks
for the ticket and spec surfaces so the cost of CLI process startup, HTTP
request handling, and MCP dispatch is measured and tracked over time.

## Scope

- CLI: cold-start + single-op latency (process spawn + parse + store op).
- HTTP: request/response latency and sustained throughput for read and
  graph endpoints (`/api/graph/subgraph`, `/api/graph/health`).
- MCP: per-tool dispatch latency for the common ticket/spec tools.

## Acceptance Criteria

- [ ] A benchmark harness measures latency (and where meaningful, throughput)
      for each transport against an isolated temp-dir store.
- [ ] Results captured as a comparable table (transport × operation × p50/p95).
- [ ] Benchmark output linked as validation evidence before `in-review`.
- [ ] Harness documents how to re-run and is referenced from the matrix index
      doc (`d8d18128`).

## Notes

- Lowest priority; depends on the E2E matrix (`c37ea985`) for the operation
  set and fixtures. Build on those rather than re-deriving the op list.
- Run sequentially on Windows to avoid Cargo build-lock contention.
- Lives in the `memory-api` submodule; commit there first, then bump pointer.
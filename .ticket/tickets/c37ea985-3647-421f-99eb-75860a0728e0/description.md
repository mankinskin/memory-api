# [profiling] CLI/HTTP/MCP end-to-end test matrix (ticket + spec surfaces)

Child of tracker `ef3f4a91`. Build a parity E2E test matrix that exercises the
same ticket and spec operations across all three transports (CLI, HTTP, MCP)
so behavior stays consistent as the profiling/bench work changes hot paths.

## Surfaces

- CLI: `memory-api/tools/cli/ticket-cli`,
  `tools/cli/spec-cli`.
- HTTP: `tools/http/ticket-http`, `tools/http/spec-http`.
- MCP: `tools/mcp/ticket-mcp`, `tools/mcp/spec-mcp`.

## Scope

- Cover a representative operation set per surface: create, get, list/search,
  update/transition, link/edge, subgraph/topgraph, health.
- Assert response parity across transports for the same logical operation
  (same ticket/spec state produces equivalent results).

## Acceptance Criteria

- [ ] An E2E matrix runs the operation set against CLI, HTTP, and MCP and
      asserts cross-transport parity.
- [ ] Tests use isolated temp-dir stores (no dependency on the live root
      `.ticket`/`.spec` stores).
- [ ] `cargo test` for the matrix passes; output linked as validation
      evidence before `in-review`.
- [ ] State transitions in tests respect the one-way state machine and
      `required_states` (visit `in-review` before terminal).

## Notes

- Depends conceptually on the native bench matrix (`6a19ae5f`) only for shared
  fixtures; can proceed independently.
- Lives in the `memory-api` submodule; commit there first, then bump pointer.
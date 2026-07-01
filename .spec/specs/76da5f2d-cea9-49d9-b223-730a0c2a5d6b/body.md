# Transport-layer e2e matrix and benchmark strategy

This spec drafts the design for a real transport matrix covering CLI, HTTP, and MCP surfaces across the in-scope memory-api domains.

## Problem

Current validation is still uneven:

- many tests exercise stores in-process only
- transport serialization and error mapping are not uniformly covered
- create/record paths can still drift toward ambient workspace assumptions if tests only hit internal APIs
- the benchmark ticket needs a transport-level operation matrix before per-operation budgets are meaningful

## Design goals

- Validate each basic operation through the real transport surface where that surface exists.
- Keep the matrix brutally honest: a missing transport is Blocked with a reason, not silently skipped.
- Separate correctness cells from performance cells, but make them share the same fixture and provenance model.
- Keep large subprocess / real-port tests rare and intentional so the matrix stays fast enough to run routinely.

## Proposed matrix shape

- Dimensions: domain × operation × transport.
- Domains: ticket, spec, rule, session, test, audit, and any HTTP surface already present for a domain.
- Operations: create, get, search/list, update, move/scan where applicable.
- Transports: cli, http, mcp.

## Concrete execution plan

### Phase 1 - transport cell registry

- Define one canonical registry in the matrix harness: `cell_id`, `domain`, `operation`, `transport`, `fixture_profile`, `expected_outcome`.
- `cell_id` format: `<domain>.<operation>.<transport>` (example: `ticket.create.mcp`).
- Add a `blocked_reason` field for unsupported transport surfaces.

### Phase 2 - correctness cells

- Implement small direct-dispatch cells for every supported transport.
- Add one fault-injection cell per transport kind proving serialization/dispatch breakage fails the cell.
- Record every cell as `ValidationExecution` with `transport`, `domain`, `operation`, `run_id`, `duration_ms`, and fixture identity.

### Phase 3 - real-process sentinel cells

- Add a curated subset of real subprocess/real-port sentinel cells:
	- CLI: spawn built binary and execute one read + one write flow.
	- MCP: run server process over stdio and execute one read + one write tool call.
	- HTTP: bind server and execute one read + one write request where HTTP exists.

### Phase 4 - benchmark coupling

- Reuse the same `cell_id` inventory for performance runs.
- Add benchmark-only metadata: iterations, warmup, percentile windows, budget thresholds.
- Emit benchmark evidence keyed by `cell_id` so correctness and performance records are joinable.

## Per-domain transport matrix (initial)

| Domain | CLI | MCP | HTTP | Required operations |
| --- | --- | --- | --- | --- |
| ticket | Yes | Yes | Yes | create, get, list/search, update, move/scan |
| spec | Yes | Yes | Yes | create, get, list/search, update, section, scan |
| rule | Yes | Yes | No | create/import, get/list/search, update, scan |
| session | Yes | Yes | No | check_in(record), lookup/get, query/list, move |
| test | Yes | Yes | No | record_spec/create, record_execution/create, get/list |
| audit | Yes | Yes | No | run/create evidence, list/query results |

Notes:

- If a transport is absent for a domain, matrix cells must be emitted as `Blocked` with explicit reason text.
- HTTP cells are only required where domain HTTP servers already exist.

## Benchmark protocol

### Commands (baseline)

- Correctness matrix:
	- `cargo test -p memory-matrix`
- Transport benchmark harness (ticket 2d59b99c):
	- `cargo test -p memory-matrix -- --ignored benchmark_transports`
- Focused transport benchmark rerun:
	- `cargo test -p memory-matrix benchmark_ticket_get_transport`

### Output contract

- Emit one row per `cell_id` with:
	- `transport`, `domain`, `operation`, `iterations`, `p50_ms`, `p95_ms`, `max_ms`, `budget_ms`, `outcome`.
- Persist to test-api executions and link the evidence ids from benchmark and matrix tickets.

## CI lane design

- Fast lane (push):
	- in-process correctness cells + minimal sentinel subprocess cells.
- Large lane (scheduled or on-demand):
	- full matrix + all sentinel transport cells + benchmark suite.
- Both lanes must report blocked cells explicitly; blocked is not pass.

## Acceptance criteria (concrete)

- At least one supported transport cell exists for every required domain operation.
- Every supported transport kind has at least one real-process sentinel cell.
- Every matrix/benchmark record is keyed by canonical `cell_id` and includes typed provenance.
- Benchmark artifacts report percentiles and budget comparisons per operation.
- CI fast/large lanes are documented and reproducible from one command each.

## Transport strategy

- Default cells should call the transport dispatch layer directly for speed and low flake.
- A small subset of large cells should invoke real subprocesses or bound ports so the matrix can catch wiring, startup, and serialization regressions.
- Missing domain/transport pairs should be recorded as Blocked with an explicit reason tied to the D8 decision in the parent validation tracker.

## Fixture strategy

- Use synthesized representative fixtures with nested workspaces and cross-store relationships.
- Seed enough entities to exercise both happy-path and boundary behavior instead of create-then-read-your-own-write smoke tests.
- Reuse one fixture family across the matrix and benchmark tickets so performance and correctness stay comparable.

## Benchmark coupling

- The benchmark track should reuse the same operation list and fixture shape as the e2e matrix.
- Benchmark outputs should report transport, operation, and duration at minimum, with p50/p95 for repeated runs where meaningful.
- Per-operation budgets should be attached to the same cells that validate correctness so regressions are visible in one place.

## Validation expectations

- The matrix should have a single documented command or harness entrypoint.
- Each execution should be captured with typed provenance, transport, duration, and fixture identity.
- The benchmark ticket should be able to reference matrix evidence instead of re-deriving the transport set.

## Implementation order

1. Finalize registry and `cell_id` schema in matrix harness.
2. Land correctness transport cells per domain.
3. Land sentinel subprocess/real-port tests.
4. Land benchmark harness using the same registry.
5. Publish matrix index doc and CI lane commands.

## Traceability

- Parent validation tracker: `memory-api/.ticket/tickets/1bc3982c-e0c2-4b6a-b809-aff4eb78d161/ticket.toml`
- Matrix ticket: `memory-api/.ticket/tickets/387843e4-815e-4424-97fa-9855a464b5e6/ticket.toml`
- Benchmark ticket: `memory-api/.ticket/tickets/2d59b99c-0205-4bf6-bad9-ecb69a52830a/ticket.toml`
- Matrix index doc ticket: `memory-api/.ticket/tickets/d8d18128-656e-4a13-9983-946d6af33c27/ticket.toml`

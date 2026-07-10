<!-- aligned-structure:v1 -->

# Summary

This spec defines the transport-matrix and benchmark policy for `memory-api` domains using the lightest identity model that still ties durable evidence back to spec compliance.

## Behavior Story

The repository should be able to prove CLI, MCP, and HTTP behavior where those surfaces exist, reuse the same operation inventory for correctness and performance, and route evidence into durable stores without inventing a new executable taxonomy too early.

## Provided Surface Contracts

- One canonical transport matrix spans correctness cells, benchmark runs, and durable evidence capture.
- `cell_id` remains the stable per-operation identity for matrix and benchmark cells.
- Existing evidence primitives stay primary: `ValidationSpec`, `ValidationExecution`, benchmark records in `test-api`, and companion `log-api` artifacts when runs emit durable runtime evidence.
- Only minimal extra metadata may be added where needed to join matrix runs, benchmark runs, and spec-compliance status: fixture identity, transport, domain, operation, run grouping, and linked evidence ids.
- Missing domain or transport coverage must remain explicit `Blocked` evidence, not silent omission.
- `log-api` is not itself a matrix domain in this spec. It is the companion evidence store for runtime-log artifacts emitted by matrix or benchmark runs, so its role must be declared explicitly rather than silently omitted.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code, schema, or API references when available.
- Validate at least one existing matrix command and one benchmark command against the lightweight evidence contract below before adding new harness shapes.

## Related Implementation Tickets

- memory-api/.ticket/tickets/1bc3982c-e0c2-4b6a-b809-aff4eb78d161/ticket.toml
- memory-api/.ticket/tickets/387843e4-815e-4424-97fa-9855a464b5e6/ticket.toml
- memory-api/.ticket/tickets/2d59b99c-0205-4bf6-bad9-ecb69a52830a/ticket.toml
- memory-api/.ticket/tickets/d8d18128-656e-4a13-9983-946d6af33c27/ticket.toml

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

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

`log-api` is intentionally outside the domain axis for this matrix because the matrix measures transport-visible correctness and performance operations for the primary domain stores. `log-api` participates as a companion evidence destination when those runs produce durable runtime-log artifacts.

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
- `log-api` coverage is still required, but as evidence-routing behavior rather than as an additional matrix domain. Missing required `log_ids` or companion runtime-log capture for runs that claim such evidence should be treated as explicit blocked or failing evidence, not silent omission.

## Benchmark protocol

## Lightweight evidence contract

This policy does not introduce a new executable-anchor taxonomy yet. It reuses existing validation and benchmark records, then adds only the smallest metadata needed to connect evidence back to matrix cells and spec compliance.

Required durable identity and linkage:

- `cell_id`: stable operation identity such as `ticket.create.mcp`
- `command`: stored on the owning `ValidationSpec` or benchmark harness spec rather than a new parallel identity object
- `fixture_profile`: stable fixture family or workspace topology label
- `transport`, `domain`, `operation`, and `run_id`: recorded on each execution or benchmark record
- `spec_ids` and `acceptance_criterion_ids`: reused wherever the run is evidence for contract compliance
- `log_ids` or other evidence ids only when a run emits companion runtime artifacts that must be queryable durably
- `budget_policy` or budget result only for benchmark-bearing runs

### Grounded examples from existing evidence

| Suite or run family | Current command | Current durable evidence | What is already good | Smallest gap to close |
| --- | --- | --- | --- | --- |
| `vt-cross-domain-matrix` | `cargo test -p memory-matrix` | `ValidationSpec` + `ValidationExecution` + linked log id `exec-vt-cross-domain-matrix-20260628-log` | Stable command, duration, transport evidence, and linked runtime log already exist | Persist one canonical `fixture_profile` and ensure `cell_id` stays stable across reruns |
| `vt-bench-matrix` | `cargo run -p memory-matrix --bin bench-matrix` | `ValidationSpec` plus `BenchmarkExecution` ingest and budget enforcement in `test-api` | Benchmark runs already distinguish operation, domain, and budget status | Reuse the same `cell_id` and require explicit spec-compliance links plus optional companion log ids where runtime evidence matters |

### Minimal metadata gaps blocking full unification

- The matrix already defines `cell_id`, but fixture identity still needs one canonical durable field.
- `ValidationExecution` captures duration, transport, operation, and `run_id`; policy must require those fields consistently instead of adding a new wrapper object.
- `BenchmarkExecution` captures numeric results and budgets; policy must only add explicit compliance and companion-evidence links where missing.
- Sentinel subprocess runs, direct dispatch cells, and Criterion-backed runs all remain valid as long as they expose the same minimal evidence fields.

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
- Persist correctness rows through `ValidationExecution` and benchmark rows through the existing benchmark execution records.
- Where a run emits logs or other durable artifacts, link those evidence ids instead of embedding a second identity layer.

## CI lane design

- Fast lane (push):
	- in-process correctness cells + minimal sentinel subprocess cells.
- Large lane (scheduled or on-demand):
	- full matrix + all sentinel transport cells + benchmark suite.
- Both lanes must report blocked cells explicitly; blocked is not pass.

## Acceptance criteria (concrete)

- At least one supported transport cell exists for every required domain operation.
- Every supported transport kind has at least one real-process sentinel cell.
- Every matrix or benchmark record is keyed by canonical `cell_id` and reuses existing durable evidence primitives rather than a new anchor taxonomy.
- Benchmark artifacts report percentiles and budget comparisons per operation.
- Evidence needed for spec compliance reuses `spec_ids`, `acceptance_criterion_ids`, `run_id`, and companion evidence links instead of inventing a heavier identity envelope.
- CI fast and large lanes are documented and reproducible from one command each.

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
- The benchmark policy should reject new suite shapes unless they declare how existing validation and benchmark records will carry `cell_id`, fixture identity, compliance links, and evidence routing.

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

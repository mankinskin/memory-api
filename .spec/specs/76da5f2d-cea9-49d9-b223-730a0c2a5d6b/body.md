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

## Traceability

- Parent validation tracker: `memory-api/.ticket/tickets/1bc3982c-e0c2-4b6a-b809-aff4eb78d161/ticket.toml`
- Matrix ticket: `memory-api/.ticket/tickets/387843e4-815e-4424-97fa-9855a464b5e6/ticket.toml`
- Benchmark ticket: `memory-api/.ticket/tickets/2d59b99c-0205-4bf6-bad9-ecb69a52830a/ticket.toml`
- Matrix index doc ticket: `memory-api/.ticket/tickets/d8d18128-656e-4a13-9983-946d6af33c27/ticket.toml`

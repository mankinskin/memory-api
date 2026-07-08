<!-- aligned-structure:v1 -->

# Summary

The shared memory workspace fixture provides deterministic, representative data for every in-scope memory domain so matrix tests, benchmark matrices, transport tests, and backfill workflows exercise seeded real data rather than throwaway self-created records.

## Behavior Story

The shared memory workspace fixture provides deterministic, representative data for every in-scope memory domain so matrix tests, benchmark matrices, transport tests, and backfill workflows exercise seeded real data rather than throwaway self-created records.

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- ticket id: 9138f4e7-2757-4d23-9676-3306608a429e

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# Representative Fixture Population

## Goal

The shared memory workspace fixture provides deterministic, representative data for every in-scope memory domain so matrix tests, benchmark matrices, transport tests, and backfill workflows exercise seeded real data rather than throwaway self-created records.

## Scope

- Populate root and nested worktree fixture stores with many internally consistent entities for tickets, specs, rules, sessions, tests, logs, docs, and audit inputs where the domain has a local store surface.
- Include cross-store references such as tickets to specs, validation executions to tickets/specs/logs, and benchmark evidence tied to domains and operations.
- Preserve nested workspace/submodule coverage from the existing fixture repository.
- Keep generation deterministic and documented so fixture contents can be regenerated and reviewed.
- Update matrix `get`, `search`, and `scan` cells to assert against seeded fixture data where a domain supports that operation.

## Non-Goals

- Adding new storage capabilities for domains that are intentionally read-only or append-only.
- Replacing focused unit tests for each domain.
- Defining final CI profile selection; that belongs to the later test-profile ticket.

## Acceptance Criteria

- Every in-scope domain store has representative seeded data across the root fixture and nested worktrees where that store exists.
- Seeded references are traversable through existing loader APIs and domain stores.
- Matrix `get`, `search`, and `scan` cells use seeded fixture data for supported operations instead of creating all data inside the cell.
- Fixture generation is deterministic and documented in the fixture README.
- Existing fixture consumers continue to pass: `cargo test -p memory-fixtures -p spec-api --test e2e_fixture_loader -p ticket-api --test e2e_fixture_loader`.
- Matrix validation continues to pass: `cargo test -p memory-matrix`.

## Traceability

- Ticket: C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/9138f4e7-2757-4d23-9676-3306608a429e
- Extends fixture ticket: C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/026b2eb6-17c6-4d02-b46b-79758f1237a1
- Consumers: transport matrix ticket `387843e4`, backfill ticket `274c5119`, scale-sensitive latency ticket `01964def`.

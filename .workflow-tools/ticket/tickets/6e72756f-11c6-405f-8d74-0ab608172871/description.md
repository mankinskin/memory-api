# Objective
Create one durable policy track for repository workflows that are currently too expensive to rediscover ad hoc:
- foundational memory-store bootstrap for new durable stores
- end-to-end benchmarking and performance evidence tied to transport correctness
- tracing instrumentation plus log, session, and execution evidence management through durable stores

## Why
The repository already has multiple mature durable-store domains and partially overlapping work on `test-api`, `log-api`, workspace resolution, and transport benchmarks. What is missing is one explicit policy track that keeps the contract simple at the base layer while still supporting richer domain workflows and cross-store composition.

## Scope
This tracker owns research and policy synthesis first, then narrows into implementation tickets once the repository agrees on the contract shape.

## Child workstreams
1. Foundational memory-store bootstrap policy with extension profiles.
2. End-to-end transport matrix and benchmark policy using lightweight evidence identity.
3. Tracing instrumentation and durable evidence-routing policy across `test-api`, `log-api`, journals, and session-style artifacts.

## Expected outputs
- new or updated workflow instructions and slash-command guidance
- required specs for contract-level behavior before implementation
- ticket decomposition for concrete code changes, templates, libraries, or migrations
- validation plan with executable commands and evidence-store expectations
- explicit dependency map to existing `memory-api`, `test-api`, `log-api`, and workspace-policy work

## Initial validation
Success for this tracker's policy phase means the child trackers exist, the durable specs and tickets encode the corrected design decisions, and the next session can move into focused implementation without re-running the broad architecture sweep.

## Synthesis snapshot (2026-07-10)

- Minimal-store slice is now being hardened around one foundational memory-store contract rather than an entity-store-versus-session split. Spec `9ee9387f-5384-42a9-95c4-ecbad1713030` now owns the core bootstrap profile, extension-profile model, and the rule that `test`, `log`, and `session` still participate in the shared foundation where they expose durable records.
- Benchmark slice is being simplified away from a premature executable-anchor taxonomy. Spec `76da5f2d-cea9-49d9-b223-730a0c2a5d6b` now keeps the transport matrix broad while preferring `cell_id`, `ValidationSpec`, `ValidationExecution`, existing benchmark records, and only minimal extra metadata for compliance and evidence routing.
- Tracing/log slice now distinguishes atomic knowledge ownership from cross-store workflow composition. Ticket `a71c2da8-0972-4c2d-9754-0a0e06db5272` carries that ownership and query model, while ticket `db9bad13-ae43-4300-8037-7165c0e9a7b0` owns the layered minimum-link interoperability contract.

## Current status by workstream

1. Foundational bootstrap policy: durable spec exists and is being corrected toward core plus extension profiles; concrete implementation ticket `e268a1e8-3f3a-433f-b4a0-d58c590b8d29` now owns the first minimal-store fixture or template smoke path.
2. Benchmark policy: durable spec exists and is being corrected toward lightweight evidence identity; next gap is enforcing fixture-profile and compliance links inside existing matrix and benchmark work.
3. Tracing and log policy: the layered minimum-link interoperability contract (`db9bad13-ae43-4300-8037-7165c0e9a7b0`) is now enforced across all five artifact classes and is `in-review`; the ownership/query-model ticket `a71c2da8-0972-4c2d-9754-0a0e06db5272` has resumed `in-implementation`.

## Immediate next implementation opportunities

- Start `e268a1e8-3f3a-433f-b4a0-d58c590b8d29` against shared `memory-api` primitives to prove the core profile by fixture or template smoke path.
- Extend existing `memory-matrix` validation and benchmark tickets with the lightweight evidence fields instead of introducing a new anchor layer.
- Advance the tracing/log ownership and query model in `a71c2da8-0972-4c2d-9754-0a0e06db5272` now that the interoperability contract it depended on is enforced and in review.

## Validation status

- `spec.exe refs 9ee9387f-5384-42a9-95c4-ecbad1713030 validate --workspace-root . --toon` -> `valid: true` after the core-profile and traceability updates.
- `spec.exe refs 76da5f2d-cea9-49d9-b223-730a0c2a5d6b validate --workspace-root memory-api --toon` -> `valid: true` after the benchmark-identity simplification.

## Interoperability contract closure (2026-07-10)

- Ticket `db9bad13-ae43-4300-8037-7165c0e9a7b0` (INTEROP) landed the final artifact-class edge — journal-backed operation lineage — enforced at the `persist_journal` persistence boundary in `memory-api`. The layered interoperability contract is now enforced at a persistence boundary across all five artifact classes: validation executions, benchmark records, log captures, runtime sessions, and journal-backed operations. INTEROP is now `in-review`.
- With INTEROP in review, the tracing/log policy ticket `a71c2da8-0972-4c2d-9754-0a0e06db5272` (POLICY) resumed from `on-hold` back to `in-implementation`.
- Validation for the closing slice: `cargo test -p test-api -p log-api -p memory-matrix` green; new `memory-api` journal-contract tests pass; one pre-existing, unrelated `ticket-api` preflight test failure (`preflight_reports_invisible_reference_visibility_and_path_refs`, ticket-store `NotFound`) confirmed on the baseline and not a regression.

## Focused health note (2026-07-10)

- Remaining dependency-state findings on this tracker and on child ticket `79dd2d35-267b-4395-8316-0761df45f3c5` are intentional tracker-convergence signals: the policy trackers remain active while newly created implementation children advance underneath them.

The tracker should now preserve the corrected design judgments so future sessions do not reintroduce the rejected assumptions.

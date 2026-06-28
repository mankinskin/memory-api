# [design][test] Browser & TypeScript automated test integration strategy (design session)

## Goal

Design how browser-automated and TypeScript-authored tests are integrated into the unified `test-api` validation surface. This is a **design/planning session**, not implementation: produce a recommended architecture and the follow-up implementation tickets.

Motivated by D7 (whole-workspace corpus backfill) and D2 (a few large e2e tests driven via a TypeScript/node runtime). Today the repo already has Playwright e2e suites (shared managed-viewer suites under `viewer-api/.../e2e/shared`, spec-viewer/ticket-viewer release e2e, doc-viewer/log-viewer wrappers) plus Rust wasm/browser tests — none flow into `test-api`.

## Questions to resolve

- How to drive TypeScript/Playwright tests from the runner harness: shell out to `npm run test:e2e`, a node-based runner, or a Rust→node runtime bridge (e.g. embedding a JS runtime)? Trade fidelity vs. toolchain weight.
- How to map TS/Playwright test results (and screenshots/artifacts) → `test-api` executions + `log-api` captures with provenance (source file, test id, transport).
- How wasm/`wasm-pack` browser tests are represented.
- Where these run in the CI lane split (D10 fast vs. on-demand): browser/e2e almost certainly on-demand/nightly.
- Whether a thin TS adapter that emits the `test-api` execution JSON schema is preferable to parsing each runner's native output.

## Deliverables

- A short design note (architecture + chosen approach + rejected alternatives).
- A set of implementation tickets created under the sub-tracker (or parent) for the chosen approach.
- A provenance mapping for TS/browser results consistent with the typed-provenance model (a03d8a97).

## Acceptance criteria

- [ ] A recommended integration approach is documented with rationale and rejected alternatives.
- [ ] Follow-up implementation tickets exist and are linked.
- [ ] The provenance/log-capture mapping for TS/browser tests is specified.

## Relationship / traceability

- Informs the corpus backfill (`274c5119`) and the TypeScript/subprocess slice of the transport matrix (`387843e4`).
- Consistent with the provenance model (`a03d8a97`) and CI-lane/test-profile work.
- This is a design ticket: it must NOT itself weaken or stub tests; failing/▢ findings it surfaces become tracked faults.

# [memory-api][test] Real end-to-end validation surface (sub-tracker)

Sub-tracker under `a0bc8bd8` (Unified validation & benchmark surface in test-api).

## Why this exists

The first wave landed a working but **unit-level** matrix (`memory-matrix`) that does not yet satisfy the parent's intent:

- Storage APIs are exercised **in-process only** — no CLI/HTTP/MCP transport, serialization, or error-mapping is covered.
- Per-cell executions are written to the **ephemeral fixture tempdir** and discarded; only an opaque roll-up is committed, with no provenance.
- The **fixture is a 4-entity stub**, so most "Passed" cells are create-then-read-your-own-write smoke checks.
- Benchmarks run on that stub: `ticket get` ≈ 347ms under a 2s budget, so the **~100s regression that motivated the parent cannot reproduce**.
- The repo's real `cargo test`/Playwright corpus is **not ingested**.

## Testing philosophy (binding for all children)

This track builds a **brutally honest test-requirement suite**. Making tests pass is **not** the goal.

- A test that fails because it uncovered a real **fault in the implementation or in a transport** is a success — leave it failing and open/track the fault; do **not** weaken, stub, or `#[ignore]` it to go green.
- **Correctness of the test itself is paramount.** A test must assert true, observable behavior end-to-end, not a tautology.
- Cells that genuinely cannot run are `Blocked` with a concrete, cited reason — **never silently skipped**, never counted as passed.
- Excluded-by-profile cells are reported as **not-run**, never as passed.

## Children

1. `387843e4` Transport-layer e2e matrix — CLI/HTTP/MCP × domains.
2. `a03d8a97` Execution provenance + persisted per-cell evidence (typed fields).
3. `9138f4e7` Representative fixture population (synthesized, many entities/edges, cross-store + nested workspaces).
4. `274c5119` Backfill existing repo test/bench suites into the store.
5. `01964def` Scale-sensitive latency fixtures + per-operation budgets.
6. `260e37d7` Audit hygiene for memory-matrix (split lib.rs, fix llvm-cov).
7. `93b8a331` Browser & TypeScript test integration strategy (design session).
8. `2dada4b7` Test profiles + CI lanes (fast-on-push vs. large-on-demand).

(`57a13857` "close transport surface gaps" was **cancelled** — building missing transports is out of scope for this track per D8; the matrix marks absent transports Blocked-with-reason.)

## Resolved decisions

| # | Decision | Resolution |
|---|---|---|
| D1 | Transport scope | CLI + MCP everywhere; HTTP only where a `*-http` surface already exists. Testing-only for now. |
| D2 | Drive mode | Mainly in-process router/handle calls; a **few** large tests via real subprocess or a TypeScript/node e2e (or Rust→node runtime). |
| D3 | Provenance | **Typed fields** on the test-api model. |
| D4 | Per-cell persistence | Persist under a run id with a **retention cap of 2 runs**. |
| D5 | Fixture realism | **Synthesize** many entities + edges, within-store and cross-store, in nested workspaces; model after the `memory-api` and `context-engine` workspaces. |
| D6 | Volume vs runtime | Pursue **completeness first**, then add test profiles to control what runs. |
| D7 | Backfill scope | **Whole workspace corpus eventually**; browser/TS integration handled by design session `93b8a331`. |
| D8 | Surface gaps | **Out of scope** for this track (ticket cancelled). |
| D9 | Budgets | **Per-operation budgets** sized to "no end-user waits longer than reasonable" — measure and budget each end-user call. |
| D10 | CI lanes | **Fast** suite on every push; **large** suite on-demand or once with a long debounce after the last push. |
| D11 | Effort | Estimated in **tokens required for implementation** (`effort` field). |

## Acceptance criteria

- [ ] Every in-scope operation is validated through at least one real transport (CLI/MCP, HTTP where present), not just in-process storage.
- [ ] Stored executions are individually discoverable and trace back to an exact test/cell/transport via typed provenance.
- [ ] The synthesized fixture populates all in-scope domains with many cross-referenced entities across nested workspaces and is consumed by the matrices.
- [ ] A scale-sensitive benchmark with per-operation budgets reproduces and would flag a 100s-class `ticket get` regression.
- [ ] Fast/large CI lanes exist; excluded cells report as not-run.
- [ ] memory-matrix passes the audit (no high findings).
- [ ] Every child carries a token `effort` estimate.

## Relationship / traceability

- Parent: `a0bc8bd8`. Extends fixture `026b2eb6`; re-validates latency fix `23f4e2eb`.
- Builds on `751f0e71`, `03ed4121`, `2b0f31e5`, `124c6621`, `7a524627`, `90de77b1`, `26d6353a`.

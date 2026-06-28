# [test] Audit hygiene for memory-matrix

## Goal

Clear the audit findings against `memory-matrix` so the crate passes a clean audit.

## Findings (from `audit run memory-api/crates/memory-matrix`)

- **high / file_length**: `src/lib.rs` is 1092 lines (> 400 limit). One file holds the trait, harness, all 8 domain impls, and the bench module.
- **high / coverage**: `cargo llvm-cov` failed (exit 101) on this crate, so coverage could not be collected.
- compiler warnings: 0 (clean).

## Scope

- Split `src/lib.rs` into feature-focused modules (harness, per-domain impls, bench_runner) each under 400 lines, keeping the public API thin.
- Root-cause and fix the `cargo llvm-cov` exit-101 failure (likely the bench/bin targets under the coverage cfg) so coverage is collectable.
- Re-run the audit and confirm no high findings remain for the crate.

## Acceptance criteria

- [ ] No file in the crate exceeds 400 lines.
- [ ] `cargo llvm-cov` collects coverage for the crate successfully.
- [ ] `audit run memory-api/crates/memory-matrix` reports no high findings for the crate.

## Relationship / traceability

- Hygiene follow-up on the in-process matrix `751f0e71` and benchmark matrix `03ed4121`.
- Should land alongside or before the transport refactor (#1) since both touch this crate.

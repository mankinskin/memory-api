## Problem

The matrix needs explicit missing-store coverage to prove strict read/search/scan paths do not recreate hidden store roots, with explicit create/init as separate positive controls.

## Implemented Reproducer Coverage

- Added: `memory-api/crates/memory-matrix/tests/missing_store_policy.rs`
- New tests:
  - `strict_read_ops_with_missing_roots_do_not_succeed_or_recreate_store`
  - `explicit_create_controls_are_the_only_root_creating_path`
- Coverage currently targets `ticket`, `spec`, and `rule` rows with missing `.ticket`, `.spec`, and `.rule` roots.

## Validation

- Command: `cargo test --manifest-path crates/memory-matrix/Cargo.toml --test missing_store_policy -- --nocapture`
- Result: **failing (expected reproducer)**
- Failure evidence: `ticket.get must not recreate missing .ticket`.
- Positive control status: explicit create controls pass (`1 passed` in same suite).

## Blocker Summary

Strict read/search/scan matrix behavior still recreates missing roots via current open helpers, so the missing-store contract is not yet enforced.

## Next Step

Split strict-open vs explicit-init semantics in matrix domain helpers and backing store calls, then re-run this suite until strict negative checks pass while explicit create controls remain green.

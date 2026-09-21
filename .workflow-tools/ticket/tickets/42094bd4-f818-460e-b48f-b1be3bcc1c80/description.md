# Query-time final guard tied to policy-allowed roots

## Goal

Harden the existing query visibility guard so it only surfaces tickets under active, policy-allowed roots — the final defense even if stale/ignored roots exist in the index.

## Current code

- `visible_scan_roots` builds the visible set from `list_scan_roots()` plus the default root — [query.rs](memory-api/crates/ticket-api/src/storage/store/query.rs#L195).
- `is_ticket_visible` filters tickets by `path.starts_with(root)` — [query.rs](memory-api/crates/ticket-api/src/storage/store/query.rs).
- Applied in `list` / `list_extended` — [query.rs](memory-api/crates/ticket-api/src/storage/store/query.rs#L28).

## Changes

1. Restrict `visible_scan_roots` to roots whose persisted `policy_decision = included` (depends on ticket 3/6 metadata).
2. Ensure `deny_external_paths` roots are never treated as visible.
3. Apply the same guard to any additional read surfaces that bypass `list` (verify search-result post-filtering and graph/subgraph reads honor the same allowed-root set).

## Non-goals

- Discovery-time filtering (ticket 2/6).
- CLI (ticket 5/6).

## Acceptance criteria

- [ ] Query surfaces exclude tickets under `ignored` / external roots even when those rows exist in the index.
- [ ] `list`, `list_extended`, and search read paths share one allowed-root computation.
- [ ] Regression test: a ticket physically under an ignored root is not returned by `list`/`search`.
- [ ] `cargo test -p ticket-api` passes.

## Files

- [memory-api/crates/ticket-api/src/storage/store/query.rs](memory-api/crates/ticket-api/src/storage/store/query.rs#L195)
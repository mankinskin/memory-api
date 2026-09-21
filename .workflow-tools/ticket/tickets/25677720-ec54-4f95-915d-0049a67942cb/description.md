# Regression tests across all enforcement points

## Goal

Prove the policy contract end to end with focused regression tests.

## Required scenarios

1. **Child included by default** — a descendant `.ticket` store with no policy/marker is discovered, scanned, and its tickets are queryable.
2. **Child ignored via marker** — a descendant containing an `ignore_markers` file (e.g. `.ticket-ignore`) is excluded at discovery, not scanned, and its tickets are not queryable.
3. **Ignored via glob** — a descendant matching an `ignore_workspaces` glob is excluded.
4. **Include override wins** — a descendant matching both `ignore_workspaces` and `include_overrides` is included.
5. **External path denied** — with `deny_external_paths = true`, an ancestor/sibling store outside the workspace root is never indexed or queryable.

Also cover:
- **Compatibility mode** — absent policy file preserves current behavior and emits the recommended-policy warning.
- **Query guard** — a ticket physically under an ignored root is filtered out of `list`/`search` even if a stale row exists.

## Scope

- Prefer native tests using `tempfile` fixtures under `memory-api/crates/memory-api` (discovery) and `memory-api/crates/ticket-api` (scan + query) mirroring existing patterns in [tests.rs](memory-api/crates/ticket-api/src/storage/tests.rs).
- Add a CLI integration test if not fully covered by ticket 5/6.
- Use `init_test_tracing!` where a store is involved.

## Acceptance criteria

- [ ] All five required scenarios plus compatibility-mode and query-guard cases have passing tests.
- [ ] `cargo test -p memory-api` and `cargo test -p ticket-api` pass.
- [ ] Evidence (commands + results) recorded on the ticket/spec before review.

## Files

- [memory-api/crates/memory-api/src/workspace.rs](memory-api/crates/memory-api/src/workspace.rs)
- [memory-api/crates/ticket-api/src/storage/tests.rs](memory-api/crates/ticket-api/src/storage/tests.rs)
- [memory-api/crates/ticket-api/src/storage/store/query.rs](memory-api/crates/ticket-api/src/storage/store/query.rs)
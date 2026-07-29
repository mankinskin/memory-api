## Problem

A stray `.rule` directory exists directly under the OS Temp root on this dev machine (`C:/Users/linus/AppData/Local/Temp/.rule/rules/`, created 2026-07-27 20:40, containing rules with slugs `shared/tests/sparse-rule` and `shared/tests/movable-rule`). Ancestor-store discovery in `memory_api::workspace` walks up from any tempdir-rooted test sandbox and picks this directory up as an `ancestor:Temp` scan root, causing two independent classes of test failure on this machine:

1. `memory-api::workspace::tests::discover_workspace_scan_roots_*` (2 tests) — assert an exact scan-root list that doesn't account for the extra ancestor entry. Verified pre-existing and unrelated during review of e82b4f88-45e1-402b-ab59-de845c4930e0.
2. `rule-mcp` integration tests `rule_update_accepts_sparse_payload_and_returns_minimal_response` and `rule_move_preflight_returns_supported_plan` (`memory-api/tools/mcp/rule-mcp/tests/smoke_test.rs`) — fail with `duplicate rule slug` because `rule_create`'s dedup check sees the same slugs already present in the ambient ancestor store. Discovered during review of 459789f8-12b7-4013-be11-521d5ca23e49.

## Scope

- Root-cause why a `.rule` directory exists directly under the OS Temp root (likely a prior test run that didn't clean up, or a test that resolves its store root incorrectly).
- Decide the fix: either (a) test isolation — tests that create tempdir-rooted sandboxes should not be susceptible to ancestor scan-root pollution from the real OS Temp root, or (b) a documented cleanup step / guard, or (c) both.
- Clean the stray directory on affected dev machines and confirm both test suites pass without it.

## Acceptance Criteria

1. `cargo test -p memory-api --lib` passes with 0 failures on a machine that previously had the stray `.rule` directory.
2. `cargo test -p rule-mcp` (including `smoke_test.rs`) passes with 0 failures under the same condition.
3. Root cause documented: why ancestor-store discovery treats a tempdir's parent chain as reaching the real OS Temp root, and whether that is correct behavior for production or only a test-isolation gap.

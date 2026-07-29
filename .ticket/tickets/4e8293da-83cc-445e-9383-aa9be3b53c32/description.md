## Problem

`cargo test -p audit-api` currently reports 13 passed / 1 failed, not the 14/0 recorded on ticket d1b3a6c9's validation evidence. The single failure:

```
test trials::rule_overlap::tests::reports_high_overlap_between_near_duplicate_rules ... FAILED
thread '...' panicked at memory-api\crates\audit-api\src\trials\rule_overlap.rs:236:36:
called `Result::unwrap()` on an `Err` value: DuplicateSlug("shared/prompts/handoff-a")
```

Repro:
```
cargo test -p audit-api --lib trials::rule_overlap::
```
Reproduces deterministically with `--test-threads=1` and when run completely alone, ruling out cross-test parallelism as the cause.

## Root cause hypothesis

The failing assertion is `store.create(&first, None).unwrap()` — the very *first* insert into a freshly created `RuleStore::init(dir.path())` backed by a `tempfile::tempdir()`. A `DuplicateSlug` on the first insert means the store's `slug_index` was already populated before any `create` call in the test, i.e. `RuleStore::init` -> `workspace::resolve_store_root_from(index_root, ".rule")` is resolving to (or importing state from) a location other than the fresh tempdir, likely by walking up parent directories or consulting a shared/global root registry rather than staying scoped to the passed-in path.

## Impact

- Blocks confident re-validation of any ticket whose evidence cites `cargo test -p audit-api` passing cleanly (e.g. d1b3a6c9), since the failure is unrelated noise that must be manually triaged out each time.
- Indicates a real test-isolation bug in `rule-api`'s store-root resolution that could also affect other tempdir-based rule-api tests, not just this one.

## Suggested next steps

- Inspect `workspace::resolve_store_root_from` and `RuleStore::open_internal` to confirm whether they can escape the requested `index_root` under any condition (e.g. ancestor-directory search, global scan-root registry, or shared static cache).
- Ensure `RuleStore::init` is fully scoped to the provided path with no fallback to ancestor or globally-registered roots for test/tempdir use.
- Add an isolation regression test asserting two `RuleStore::init` calls against distinct tempdirs never see each other's slugs.

## Evidence

- Failing test: [rule_overlap.rs](memory-api/crates/audit-api/src/trials/rule_overlap.rs#L214-L236)
- Surfaced while reviewing ticket d1b3a6c9 "Route workflow diagnostics upward and add structural workflow-graph validation" — this ticket's own new `session_workflow_graph` trial test passes in isolation, so the failure is unrelated to that ticket's diff.
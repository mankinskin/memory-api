# [memory-api] E2E test workspace fixture repository — multi-store, multi-submodule

## Goal

Provide a dedicated, version-controlled **fixture repository** that materializes a realistic multi-store, multi-worktree workspace so every memory tool (ticket, spec, rule, audit, session, test, doc, log) and all cross-store / cross-worktree operations (notably `move`) can be exercised **end-to-end** and **benchmarked** against stable, known data with documented invariants.

## Problem / current state

- Cross-worktree/submodule behavior is currently tested with per-test `git init` tempdir repos (see `execute_move_with_journal_rewrites_parent_repo_refs_for_submodule_source` in [move_execution.rs](memory-api/crates/ticket-api/src/storage/move_execution.rs)). This is fragile, duplicated across crates, and does not reflect real git submodule pointer mechanics.
- The recently fixed Windows verbatim-path bug (`\\?\` leaking into journals/rewrites/validation) was only caught manually against the live repo, not by an E2E test, because the unit tests bypass the CLI canonicalization boundary.
- There is no shared, sizeable, reproducible dataset for benchmarks (`memory-api/test-fixtures/` holds a single JSON contract file only).
- Every domain (8 stores) shares the same workspace shape but each maintains ad-hoc setup.

## Scope

A fixture repo plus a loader API consumed by tests and Criterion benchmarks across all memory tools.

### Fixture repository layout

```
memory-workspace-fixture/              (root git repo)
├── .ticket/ .spec/ .rule/ .audit/ .session/ .test/ .doc/ .log/   (root-level stores)
├── submodule-a/                       (git submodule — separate worktree)
│   └── .ticket/ .spec/ ...            (nested stores)
├── submodule-b/                       (git submodule)
│   └── .ticket/ ...
└── fixtures.toml                      (manifest: store inventory, entity counts,
                                        cross-store + cross-worktree reference map,
                                        expected post-operation invariants)
```

### Seeded scenarios

- Same-store references (ticket→ticket, spec→spec).
- Cross-store references within one worktree (ticket↔spec↔rule↔test↔audit).
- **Cross-worktree references** (root-store entity ↔ submodule-store entity) for visibility checks and move rewrites.
- Path references in tracked text files (markdown bodies, README, `index.toon`) citing entity folder paths — exercises rewrite + manual-followup detection.
- Board entries / leases in active, stale, and historical states — exercises fail-closed and historical migration.
- Clean vs. dirty working-tree variants.
- Dataset sizes: a **small deterministic** variant (correctness) and a **large generated** variant (benchmark scale).

### Loader API

A new `memory-fixtures` crate (under `memory-api/crates/`) that:
- copies/clones the fixture repo into an isolated tempdir and initializes submodules (pinned SHAs) so tests mutate freely without dirtying the source;
- returns resolved store roots per domain + per worktree, plus the parsed `fixtures.toml` manifest;
- exposes assertion helpers to verify post-operation invariants from the manifest;
- provides a generator for the large/benchmark variant.

## Non-goals

- Replacing all existing focused unit tests; the fixture complements them for E2E + benchmarks.
- Network access; submodules are local/pinned.
- Tool-specific golden output snapshots beyond the shared invariants manifest.

## Acceptance criteria

- [ ] Fixture repo exists with a root repo + ≥2 submodules; each worktree contains ≥2 store types with seeded entities and a `fixtures.toml` manifest.
- [ ] `memory-fixtures` loader materializes the fixture into a tempdir with submodules initialized and returns store roots + manifest.
- [ ] Cross-worktree move E2E test (parent↔submodule) runs against the fixture and asserts: rewritten paths contain **no `\\?\` / `//?/` prefix**, correct journal phases, rollback restoration, and manual-followup detection.
- [ ] At least one additional memory tool (e.g. spec or rule) has an E2E test consuming the fixture.
- [ ] A Criterion benchmark runs an operation (scan / search / move-plan) over the large dataset variant.
- [ ] Usage is documented (how to add a store type or scenario, how to regenerate the large variant).

## Open decisions

- Host the fixture as a real git submodule of this repo vs. a committed data directory plus a scaffolding generator. Recommendation: committed fixture repo (submodule) for realism + a generator for the large/benchmark variant.
- Exact `fixtures.toml` schema for the invariants map.

## Relationship / traceability

- Directly supports `21e6c015` (cross-git-worktree move) E2E coverage and would have caught the `\\?\` regression.
- Supports the generic move kernel `0a510279` and all domain api crates' integration tests.
- First consumer scenario: the parent↔submodule ticket move (the `2b1279bd` / `671d4e47` real-world case).

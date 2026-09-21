# Apply policy in discover_workspace_scan_roots (discovery filtering)

## Goal

Make `discover_workspace_scan_roots` policy-aware so unwanted workspaces are never added as scan roots.

## Current code

`discover_workspace_scan_roots` — [workspace.rs](memory-api/crates/memory-api/src/workspace.rs#L476) — unconditionally:
1. collects descendants via `find_descendant_store_roots_from`,
2. folds in ancestor stores (`ancestors().skip(1)`),
3. relies only on `should_skip_descendant_dir` — [workspace.rs](memory-api/crates/memory-api/src/workspace.rs#L625).

## Algorithm to implement

Given the resolved `WorkspacePolicy` (ticket 1/6):

1. Start from the active workspace root.
2. Discover descendants only if `include_descendants` is true.
3. Exclude any candidate workspace matching `ignore_workspaces` or containing an `ignore_markers` file.
4. Re-include a previously-excluded candidate only if matched by `include_overrides`.
5. When `deny_external_paths` is true, never include a store root outside the workspace root (this constrains the ancestor branch).
6. Include ancestors only if `include_ancestors` is true and not ignored.

## Scope

- Add a policy-aware entry point (e.g. `discover_workspace_scan_roots_with_policy`) or thread a `&WorkspacePolicy` parameter; keep a thin compatibility wrapper for existing callers until they are migrated.
- Callers to review: `rule-api`, `spec-api`, `ticket-api` store bootstrap, and CLI dispatch paths that call `discover_workspace_scan_roots`.

## Non-goals

- Persisting policy metadata (ticket 3/6).
- Query-time guard (ticket 4/6).

## Acceptance criteria

- [ ] Descendant discovery is gated on `include_descendants`.
- [ ] Ancestor inclusion is gated on `include_ancestors` and suppressed under `deny_external_paths`.
- [ ] `ignore_workspaces` globs and `ignore_markers` files exclude candidates; `include_overrides` re-includes them.
- [ ] Existing callers compile via the compatibility wrapper with no behavior change when no policy file exists.
- [ ] Unit tests cover each branch; `cargo test -p memory-api` passes.

## Files

- [memory-api/crates/memory-api/src/workspace.rs](memory-api/crates/memory-api/src/workspace.rs#L476)
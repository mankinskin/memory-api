# Policy file parser + in-memory policy object

## Goal

Add a `WorkspacePolicy` type parsed from `.ticket/workspace-policy.toml`, with documented defaults and an absent-file compatibility mode.

## Schema

```toml
include_descendants = true      # default
include_ancestors   = true      # safer default
deny_external_paths = true      # hard security boundary
ignore_workspaces   = ["glob-or-relative-path", ...]
include_overrides   = ["glob-or-relative-path", ...]
ignore_markers      = [".ticket-ignore", ".workspace-ignore"]
```

## Scope

- New module (e.g. `workspace_policy.rs`) in `memory-api` alongside [workspace.rs](memory-api/crates/memory-api/src/workspace.rs) exposing `WorkspacePolicy` (serde `Deserialize`) with `#[serde(default)]` field defaults.
- Loader `load_workspace_policy(workspace_root) -> WorkspacePolicy` that:
  - reads `<workspace_root>/.ticket/workspace-policy.toml` when present (authoritative),
  - returns compatibility-mode defaults when absent (`include_descendants=true`, `include_ancestors=true` for parity with current behavior) and emits a `tracing::warn!` recommending an explicit policy exactly once per resolution.
- Helper predicate on the policy object for matching a candidate path against `ignore_workspaces` / `include_overrides` globs and detecting `ignore_markers` files (glob matching via an existing workspace dependency; confirm which crate is already vendored before adding a new one).

## Non-goals

- Wiring into discovery, scan, or query paths (later tickets).
- CLI surfaces (later ticket).

## Acceptance criteria

- [ ] `WorkspacePolicy` deserializes a full policy file and applies documented per-field defaults for partial files.
- [ ] Absent file yields compatibility-mode defaults and emits a single warning.
- [ ] Glob/marker matching helpers are unit-tested (match, non-match, override precedence).
- [ ] `cargo test -p memory-api workspace_policy` passes.

## Files

- [memory-api/crates/memory-api/src/workspace.rs](memory-api/crates/memory-api/src/workspace.rs) (module wiring)
- new `memory-api/crates/memory-api/src/workspace_policy.rs`
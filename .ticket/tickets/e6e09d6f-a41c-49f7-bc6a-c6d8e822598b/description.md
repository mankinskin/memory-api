# Normalize nested workspace option semantics across ticket/spec/rule CLIs

## Review summary

The ticket direction is right, but the existing plan was too broad to execute safely. The core contract already lives in the workspace design spec, and the remaining implementation should be treated as a narrow CLI option-normalization pass plus focused regressions.

Current gap:

- `ticket-cli` and `spec-cli` already expose global store-selection options and route them through shared workspace resolution.
- `rule-cli` now exposes a matching global `--workspace-root`, but the plan still needs to verify it behaves like ticket/spec for read/query/scan surfaces.
- `spec-cli refs validate` correctly uses command-local `--code-workspace-root`, but a previous Windows review failure showed inconsistent path normalization in the JSON payload for explicit code roots.
- `spec-cli bootstrap` uses command-local `--source-workspace-root`; this must stay distinct from global `--workspace-root` so source relativization does not collide with store selection.
- `ticket move`, `spec move`, and `rule move` all use command-local `--to-workspace-root`; this ticket should explicitly confirm those flags remain destination selectors, not active store selectors.

## Design spec

Owning design spec: `ae5ef697-0ee5-4f74-9dca-2cb268290dae` (`memory-api/workspace`).

Specific contract sections to preserve:

- `Workspace contract`: repository roots, hidden-store roots, and paths inside hidden stores normalize to one owning store when they refer to the same workspace.
- `Downstream CLI contract`: global `--workspace-root` selects the active nested workspace; global `--index-root` is the hidden-store override and wins over `--workspace-root`; command-local code/source/destination roots must not reuse global store-selection semantics.
- `Workspace fixture strategy`: validation must cover local repo-root entry, direct hidden-store entry, in-store descendant paths, nested child workspaces, explicit child workspace roots, and non-workspace fallback paths.
- `Validation expectations`: focused tests must cover ticket/spec/rule global workspace-root targeting, root-level nested `spec refs validate`, and search/list/get consistency for descendant scan roots.

Do not create a new spec for this ticket unless the public contract changes. If implementation reveals a missing rule, update `memory-api/workspace` directly.

## Implementation checklist

### Shared resolver behavior

- Confirm `crates/memory-api/src/workspace.rs` is the single source for store-root normalization, especially `resolve_store_root_from`, `resolve_workspace_root_from_store_root`, `discover_workspace_scan_roots`, and Windows path normalization helpers.
- Add or tighten helper-level tests for these equivalence classes: repo root -> hidden store, hidden store -> hidden store, hidden-store descendant -> owning hidden store, parent root + explicit child workspace -> child hidden store, and explicit non-workspace scratch path -> preserved fallback.
- Keep public aliases such as `default`, `..`, and empty roots out of the concrete CLI contract unless a command explicitly owns internal alias handling.

### `ticket-cli`

- Verify global flags in `tools/cli/ticket-cli/src/cli.rs`: `--workspace-root` and `--index-root` feed `cli::dispatch` as store selectors.
- Verify `tools/cli/ticket-cli/src/cli/dispatch.rs` resolves precedence consistently: explicit `--index-root` > explicit `--workspace-root` > cwd/PWD discovery.
- Preserve existing tests around `resolve_index_root_prefers_explicit_workspace_root`, `resolve_index_root_prefers_explicit_index_root_over_workspace_root`, and explicit child-workspace `get`/`search`/`list`/`scan` dispatch.
- Confirm `tools/cli/ticket-cli/src/cli/args/operations.rs` and `commands/lifecycle.rs` keep `--to-workspace-root` as a move destination selector, not an active store selector.

### `spec-cli`

- Verify global flags in `tools/cli/spec-cli/src/cli.rs` feed `tools/cli/spec-cli/src/cli/dispatch.rs` as store selectors with the same precedence as ticket-cli.
- Keep `tools/cli/spec-cli/src/cli/args.rs` command-local flags distinct:
	- `refs validate --code-workspace-root <PATH>` resolves code-reference files.
	- `bootstrap --source-workspace-root <PATH>` controls source-path relativization.
	- `move --to-workspace-root <PATH>` selects a destination workspace.
- Fix the known Windows review failure in `tools/cli/spec-cli/src/cli/commands/refs.rs`: payload paths for explicit `code_workspace_root` must normalize to the same slash/canonical representation the test expects.
- Preserve/extend tests in `tools/cli/spec-cli/src/cli.rs`, `tools/cli/spec-cli/src/cli/dispatch.rs`, and `tools/cli/spec-cli/src/cli/commands/refs.rs`, especially `parse_refs_validate_keeps_workspace_root_meanings_distinct`, `parse_bootstrap_uses_source_workspace_root_name`, `dispatch_get_reads_child_spec_from_explicit_workspace_root`, `dispatch_search_reads_child_spec_from_explicit_workspace_root`, `dispatch_scan_registers_child_spec_from_explicit_workspace_root`, `refs_validate_prefers_explicit_code_workspace_root`, and `refs_validate_uses_owning_workspace_for_nested_spec`.

### `rule-cli`

- Verify global flags in `tools/cli/rule-cli/src/cli/args.rs` expose `--workspace-root` with the same meaning as ticket/spec.
- Verify `tools/cli/rule-cli/src/cli/helpers.rs` uses `resolve_index_root` and `resolve_workspace_root` consistently with `memory_api::workspace::resolve_store_root_from` and `resolve_workspace_root_from_store_root`.
- Verify `tools/cli/rule-cli/src/cli/dispatch.rs` uses the resolved workspace root for descendant scan-root discovery and read/query/scan behavior.
- Preserve `move --to-workspace-root` as a destination selector in `tools/cli/rule-cli/src/cli/dispatch.rs`.
- Add or tighten rule-cli tests mirroring ticket/spec: explicit child workspace root for `get`, `search`, `list` or equivalent query, and `scan`; `--index-root` should override `--workspace-root` where both exist.

## E2E validation strategy

This ticket should not stop at helper/unit tests. It should become one focused nested-workspace validation lane that can later feed the broader transport matrix.

### Focused command validation

Run from the `context-engine` repo root, targeting the child `memory-api` workspace explicitly:

```bash
cargo test -p memory-api workspace -- --nocapture
cargo test -p ticket-cli workspace_root -- --nocapture
cargo test -p spec-cli workspace_root -- --nocapture
cargo test -p spec-cli refs_validate -- --nocapture
cargo test -p rule-cli workspace_root -- --nocapture
./target/debug/spec.exe --workspace-root memory-api refs ae5ef697-0ee5-4f74-9dca-2cb268290dae validate --code-workspace-root memory-api --json
```

Expected assertions:

- `ticket-cli`, `spec-cli`, and `rule-cli` resolve `--workspace-root memory-api` to `memory-api/.ticket`, `memory-api/.spec`, and `memory-api/.rule` respectively.
- `--index-root` remains an explicit hidden-store override and wins over global `--workspace-root`.
- `spec refs validate` returns `valid: true` for `ae5ef697` from the parent repo root and reports `workspace_root` using normalized slash paths.
- `spec refs validate --code-workspace-root memory-api` validates code refs relative to the memory-api code tree, not the parent repo root.
- Search/list/get behavior agrees for child entities reached from the parent root.

### E2E/matrix integration

- Add a validation spec/execution in `test-api` for this lane, e.g. `vt-nested-workspace-cli-options`, linked to ticket `e6e09d6f` and spec `ae5ef697`.
- Feed the same scenario into the transport strategy owned by `387843e4` when practical: CLI transport should exercise ticket/spec/rule commands from an ancestor repo root with explicit `--workspace-root memory-api`; MCP/HTTP should either prove equivalent concrete workspace-root inputs or record an explicit blocked reason if that transport does not expose the selector yet.
- Keep the lane separate from `memory-matrix`'s generic domain-operation matrix unless/until `memory-matrix` grows a workspace-topology axis. The useful E2E signal here is not just `get/search`, but parent-root invocation plus child-workspace selection plus command-local flag disambiguation.
- Record validation evidence with command, cwd, OS/path normalization note, and linked ticket/spec ids so the previous Windows slash-normalization regression cannot silently reappear.

## Completion checklist

- [ ] `memory-api/workspace` remains the owning design spec and is updated only if the public contract changes.
- [ ] Ticket/spec/rule CLIs share the same active-store selector contract: `--workspace-root`, `--index-root`, precedence, and nested child resolution.
- [ ] Command-local flags remain distinct: `--code-workspace-root`, `--source-workspace-root`, and `--to-workspace-root` never double as active store selectors.
- [ ] The known Windows `spec refs validate` path-normalization failure is fixed and covered by a focused regression.
- [ ] Root-level commands from `context-engine` can target `memory-api` without spelling hidden store directories.
- [ ] Focused tests pass for ticket/spec/rule workspace-root behavior and spec refs validation.
- [ ] E2E validation evidence is recorded or explicitly queued under `test-api` / `387843e4` with ticket `e6e09d6f` and spec `ae5ef697` traceability.
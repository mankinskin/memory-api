## Objective

Remove the legacy `.session/runtime/` fallback shims now that the flattened `.session/sessions/<id>/` layout is canonical and the legacy tree has been deleted after byte-and-hash verification.

Back-compat for `runtime/workspaces/` was deliberately dropped by user decision during the iteration review of `7a4f9c3d`. That decision amended `7a4f9c3d` AC3 and `0a45bedb` AC5 to no longer require fail-open legacy reads; this ticket carries out the corresponding code removal.

## Scope

- Remove `legacy_runtime_paths_for_workspace` and `legacy_active_workspace_session_path` from `memory-api/crates/session-api/src/store/config/persistence.rs`.
- Remove the legacy fallback branches that call them, notably the `NotFound` fallback in `read_runtime_context` in `memory-api/crates/session-api/src/store/config/worktree_runtime.rs`.

## Acceptance Criteria

1. No symbol named `legacy_*` remains in the session store config module.
2. No code path reads `.session/runtime/`.
3. The `session-api`, `session-mcp`, `session-cli`, and `ticket-api` suites still pass.

## Notes

Other worktrees or machines may still hold a `.session/runtime/` tree. Removal is fail-closed by design per the user decision; stale trees are abandoned, not migrated.

## Incident (2026-07-28)

- On 2026-07-28 a stale `~/.cargo/bin/session-mcp.exe` (installed before the session-store flatten commit) wrote a handoff to the legacy `.session/runtime/workspaces/<id>/` layout, recreating a tree that had been deleted.
- Source code was verified correct; this was a tooling-install staleness issue, not a code regression.
- Consequence for this ticket: when removing the legacy fallback shims, ALSO sweep any residual `.session/runtime/` tree, and consider adding a startup assertion or test that fails loudly if a legacy path is ever written, so stale-binary drift is detected rather than silently tolerated.
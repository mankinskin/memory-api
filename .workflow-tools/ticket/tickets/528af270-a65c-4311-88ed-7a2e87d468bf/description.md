# [workspace-policy] 7/6 Fixture boundary cleanup + policy-excluded reference audit guard

## Why

Workspace-policy rollout should prevent future indexing/visibility of excluded workspaces, but the store currently contains historical "retro" tickets and cross-workspace edges that reference fixture workspace tickets. Those references should be removed so ticket-graph/audit signals represent only intended workspace scope.

## Scope

1. Remove introduced retrospective/retro tickets that were created only to satisfy orphan cleanup for fixture stores.
2. Remove **all** graph edges where:
   - edge target ticket is under a policy-excluded workspace (fixture/test workspace), and
   - edge source ticket is outside that excluded workspace.
3. Keep fixture-internal graph relationships intact (inside→inside edges remain allowed).
4. Implement audit detection to fail when any policy-excluded workspace ticket is referenced by tickets outside those excluded workspaces.

## Audit rule contract

- Trial: `ticket_graph`
- Metric name: `policy_excluded_reference_count`
- Threshold: `0`
- Finding category: `ticket_graph`
- Evidence includes:
  - `edge_kind`
  - `source_ticket_id`, `source_path`, `source_workspace_root`
  - `target_ticket_id`, `target_path`, `target_workspace_root`
  - policy exclusion reason / matched rule

## Acceptance criteria

- [x] No retro cleanup tickets remain that were added solely to bridge fixture-store orphan findings.
- [x] No non-fixture tickets reference fixture tickets via any edge kind.
- [x] `audit run` reports `policy_excluded_reference_count = 0` for a clean store slice after reconciliation.
- [x] Regression test covers at least one violating edge and one allowed fixture-internal edge.
- [x] Ticket description records exact cleanup commands and before/after evidence artifact paths.

## Notes

- This ticket is a follow-on hardening/cleanup slice under the workspace-policy tracker.
- Execute cleanup after policy enforcement slices are merged to avoid reintroducing references during rescan/reindex.

## 2026-07-07 Resolution Log (Step 3)

### Retro/deleted dependency cleanup

- Removed stale deleted-cluster depends_on edges:
  - `e7c593dd -> 95f4e820`
  - `e7c593dd -> a6110ca3`
  - `e7c593dd -> b771d190`
  - `cc8e9a36 -> 6fceddbd`
  - `cc8e9a36 -> 712097d7`
- Reconciled store using `ticket scan --workspace . --force --toon`.
- Verified removed IDs no longer appear in target manifests and no feature tickets remain parented by audit batch tickets.

### Memory-api residual diagnostic hardening

- Added explicit policy file:
  - `memory-api/.ticket/workspace-policy.toml`
  - `include_descendants = true`
  - `include_ancestors = false`
  - `deny_external_paths = true`
- Implemented scan artifact pruning for empty UUID folders missing `ticket.toml`:
  - code: `memory-api/crates/ticket-api/src/storage/ticket_fs.rs`
  - test: `scan_force_prunes_empty_uuid_artifact_folder_without_manifest` in `memory-api/crates/ticket-api/src/storage/tests.rs`
- Validation:
  - `cargo test -p ticket-api scan_force_prunes_empty_uuid_artifact_folder_without_manifest -- --nocapture`
  - `cargo test -p ticket-api scan_force_prunes_row_for_physically_removed_ticket -- --nocapture`
  - `cargo run --manifest-path memory-api/tools/cli/ticket-cli/Cargo.toml -- workspace policy show --workspace memory-api --toon`
  - `cargo run --manifest-path memory-api/tools/cli/ticket-cli/Cargo.toml -- workspace rescan --workspace memory-api --apply-policy --toon`
  - `cargo run --manifest-path memory-api/tools/cli/ticket-cli/Cargo.toml -- scan --workspace memory-api --force --toon`
- Final evidence: memory-api force scan now returns `diagnostics[0]`.

### Linked tracker chain notes

- Parent roadmap tracker: `edde88d6` (full audit remediation)
- Retro tooling stream tracker: `e7c593dd`
- Historical batch reference only: `cc8e9a36`
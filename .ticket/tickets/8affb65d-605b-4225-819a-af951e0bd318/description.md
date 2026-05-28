# Problem

`ticket-viewer` needed a viewer-local workaround to start from a checkout where
`.ticket/tickets/...` manifests exist but the derived SQLite/Tantivy index
artifacts do not. That behavior belongs in the shared memory-api store layer so
local callers can bootstrap manifest-only workspaces without copying the same
`init + scan(true)` logic per binary.

## Scope

- add a shared `open_or_init(...)` bootstrap helper to the local store wrappers
  that currently split strict `open(...)` from idempotent `init(...)`
- preserve strict `open(...)` semantics for callers that intentionally treat a
  missing index as an error
- rebuild manifests into the derived index only when bootstrapping a missing
  local index, not on every successful open
- switch `ticket-viewer` to the shared store helper and drop the viewer-local
  rebuild logic
- add focused regression tests for `ticket-api`, `spec-api`, `rule-api`, and
  the `ticket-viewer` startup path

## Acceptance criteria

- `TicketStore`, `SpecStore`, and `RuleStore` expose a shared
  `open_or_init(...)` path that succeeds for manifest-only local workspaces
- `open(...)` in those stores still returns `WorkspaceNotFound` when callers
  explicitly require a pre-initialized workspace
- when `open_or_init(...)` bootstraps a missing index, it force-scans manifests
  so existing entities become queryable immediately
- `ticket-viewer` uses the shared store helper instead of a local `init` /
  `scan(true)` sequence

## Validation

- Passed: `cargo test -p ticket-api open_or_init_bootstraps_manifest_only_workspace -- --nocapture`
- Passed: `cargo test -p ticket-api open_rebuilds_existing_empty_index_from_manifests -- --nocapture`
- Passed: `cargo test -p ticket-cli dispatch_list_repairs_existing_empty_root_index -- --nocapture`
- Passed: `cargo test -p ticket-viewer --bin ticket-viewer startup_rebuilds_existing_empty_ticket_index -- --nocapture`
- Passed: `cargo test -p spec-api open_or_init_bootstraps_manifest_only_local_store -- --nocapture`
- Passed: `cargo test -p rule-api open_or_init_bootstraps_manifest_only_local_store -- --nocapture`
- Passed: `cargo test -p ticket-viewer --bin ticket-viewer startup_bootstraps_manifest_only_ticket_store -- --nocapture`

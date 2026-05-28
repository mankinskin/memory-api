# store bootstrap open

## Problem

The local memory-api store wrappers currently expose two low-level entry points:

- strict `open(...)`, which rejects a missing local index with
  `WorkspaceNotFound`
- idempotent `init(...)`, which creates the derived index artifacts when they do
  not exist

That split is useful for CLI and automation callers that want explicit failure,
but it leaves local servers and viewers to reimplement the same bootstrap logic
when a repository already contains manifest folders and only the derived index
artifacts are missing.

## Requirements

- `TicketStore`, `SpecStore`, and `RuleStore` must provide a shared
  `open_or_init(...)` helper for local workspaces.
- `open_or_init(...)` must preserve the behavior of strict `open(...)` for
  already-initialized workspaces by opening the existing index without forcing a
  full rebuild.
- If strict `open(...)` would fail only because the local derived index is
  missing, `open_or_init(...)` must create the index artifacts and then run a
  force scan so manifest-backed entities become queryable immediately.
- Store wrappers whose manifests live outside the generic `entities/` default
  scan root must register their canonical manifest directory before relying on
  `open_or_init(...)` rebuilds.
- `open(...)` must remain strict and continue returning `WorkspaceNotFound` for
  callers that intentionally require a pre-initialized workspace.
- Downstream local binaries may use `open_or_init(...)` to avoid duplicating
  local bootstrap logic.

## Non-Goals

- silently changing strict `open(...)` semantics for all callers
- forcing a scan on every successful `open_or_init(...)` call when the index is
  already present
- changing workspace-root resolution rules for `.ticket`, `.spec`, or `.rule`

## Acceptance Criteria

- `TicketStore::open_or_init(...)`, `SpecStore::open_or_init(...)`, and
  `RuleStore::open_or_init(...)` succeed for manifest-only local workspaces.
- Entities already present on disk are queryable immediately after
  `open_or_init(...)` bootstraps a missing index.
- Existing callers that depend on `open(...)` returning `WorkspaceNotFound` keep
  that behavior unchanged.

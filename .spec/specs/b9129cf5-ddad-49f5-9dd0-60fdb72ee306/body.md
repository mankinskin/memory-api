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
- `open_or_init(...)` and companion bootstrap helpers must not create a missing
  local store root directory. If `.ticket`, `.spec`, `.rule`, `.test`, or
  `.log` does not already exist at the resolved root, callers must receive the
  same not-found contract they would have received from strict `open(...)`.
- `open_or_init(...)` must preserve the behavior of strict `open(...)` for
  already-initialized workspaces by opening the existing index without forcing a
  full rebuild.
- If the local derived index is missing, or if a previously created local
  index opens empty while manifest-backed entities already exist on disk,
  store bootstrap must run a force scan so manifest-backed entities become
  queryable immediately.
- Store wrappers whose manifests live outside the generic `entities/` default
  scan root must register their canonical manifest directory before relying on
  `open_or_init(...)` rebuilds.
- `open(...)` must remain strict and continue returning `WorkspaceNotFound` for
  callers that intentionally require a pre-initialized workspace.
- Read/discovery/open flows that resolve a workspace root (including HTTP server
  startup and tool-hosted server launch paths) must remain side-effect free when
  the resolved local store root is absent.
- Downstream local binaries may use `open_or_init(...)` to avoid duplicating
  local bootstrap logic.

## Non-Goals

- silently changing strict `open(...)` semantics for all callers
- forcing a scan on every successful `open_or_init(...)` call when the index is
  already present
- auto-creating missing `.ticket`, `.spec`, `.rule`, `.test`, or `.log` roots
  from read-only or server-start entry points
- changing workspace-root resolution rules for `.ticket`, `.spec`, or `.rule`

## Acceptance Criteria

- `TicketStore::open_or_init(...)`, `SpecStore::open_or_init(...)`, and
  `RuleStore::open_or_init(...)` succeed for manifest-only local workspaces
  where the local hidden store root already exists.
- Entities already present on disk are queryable immediately after
  `open_or_init(...)` bootstraps a missing index.
- Existing callers that depend on `open(...)` returning `WorkspaceNotFound` keep
  that behavior unchanged.
- HTTP memory-api E2E coverage proves server startup in a repository with no
  `.ticket` root does not create one implicitly.
- Ticket VS Code launch coverage proves no-`.ticket` local server launch keeps
  the workspace untouched unless an explicit init command is invoked.
- Memory matrix coverage proves missing-store cases remain not-found for
  strict read/open/discovery flows and that explicit init remains the only
  root-creating path.
- Positive controls prove explicit init paths still create and bootstrap local
  store roots when called directly.

## Traceability

- Tracker: `C:/Users/linus/git/graph_app/context-engine/.ticket/tickets/e6bdafbe-3538-47a3-8837-1f8e74fb13e8/ticket.toml`
- Spec ticket: `C:/Users/linus/git/graph_app/context-engine/.ticket/tickets/a9514081-35c2-4162-b62d-3baf4a14ec8b/ticket.toml`
- HTTP validation ticket: `C:/Users/linus/git/graph_app/context-engine/.ticket/tickets/23f1c81b-3c71-4b4b-9e6f-81ee7c43a30b/ticket.toml`
- VS Code launch validation ticket: `C:/Users/linus/git/graph_app/context-engine/.ticket/tickets/50307cce-5a93-4668-9481-a3af5985ea1b/ticket.toml`
- Memory matrix validation ticket: `C:/Users/linus/git/graph_app/context-engine/.ticket/tickets/51e2210c-829b-4f7f-865e-99d120d8fd7d/ticket.toml`

## Validation Evidence Plan

- HTTP E2E run for missing `.ticket` server-start behavior and no-implicit-init
  assertions.
- Ticket VS Code launch reproducer for no-`.ticket` startup behavior.
- Memory matrix run covering missing-store policy and explicit-init positive
  controls.
- Final evidence links recorded after execution using the validation store and
  referenced from this spec ticket cluster.

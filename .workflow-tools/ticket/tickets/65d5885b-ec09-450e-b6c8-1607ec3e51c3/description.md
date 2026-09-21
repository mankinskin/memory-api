# Tracker: Explicit workspace-policy layer for scan-root discovery

## Problem

There is no explicit workspace-policy layer today — only discovery behavior plus scan roots. Because of this, indexed tickets can leak when discovery or stored roots pull in workspaces that were not intended (e.g. fixture/test workspaces, ancestor stores, sibling checkouts).

### Current behavior (grounded in code)

- Descendants are discovered recursively by default via `find_descendant_store_roots_from` inside `discover_workspace_scan_roots` — [workspace.rs](memory-api/crates/memory-api/src/workspace.rs#L476).
- Ancestor stores are also folded in during discovery (`for ancestor in workspace_root.ancestors().skip(1)`) — [workspace.rs](memory-api/crates/memory-api/src/workspace.rs#L482-L489).
- Only a hardcoded skip list is applied (`.git`, `.hg`, `.svn`, `target`, `node_modules`, `release`, `tmp`) via `should_skip_descendant_dir` — [workspace.rs](memory-api/crates/memory-api/src/workspace.rs#L625).
- There is no first-class allow/deny policy for child workspaces.
- Query surfaces read every indexed ticket unless filtered by root visibility. A partial guard already exists in `visible_scan_roots` / `is_ticket_visible` — [query.rs](memory-api/crates/ticket-api/src/storage/store/query.rs#L195).
- `ScanRoot` currently only carries `{ path, label }` — [filesystem.rs](memory-api/crates/memory-api/src/model/filesystem.rs#L19) — and the `scan_roots` table stores only `(path, label)` — [auxiliary.rs](memory-api/crates/memory-api/src/storage/index/auxiliary.rs#L48).

## Goal

Introduce a first-class, opt-out workspace policy (`.ticket/workspace-policy.toml`) that governs which workspaces are discovered, scanned, and queried. Children are included by default; any child can opt out explicitly; fixture/test workspaces can be denied permanently; external roots cannot be indexed when `deny_external_paths` is enabled.

## Proposed policy schema (`.ticket/workspace-policy.toml`)

- `include_descendants = true` (default)
- `include_ancestors = false` (safer default; compatibility mode may keep `true`)
- `ignore_workspaces = ["glob-or-relative-path", ...]`
- `include_overrides = ["glob-or-relative-path", ...]`
- `deny_external_paths = true` (hard security boundary)
- `ignore_markers = [".ticket-ignore", ".workspace-ignore"]`

## Enforcement points (all three must enforce policy)

1. **Discovery time** — before roots are added.
2. **Scan time** — ignore roots not allowed by policy.
3. **Query time** — final guard: only tickets under active allowed roots.

## Safety guarantees delivered

- Child workspaces included by default.
- Any child workspace can opt out explicitly (glob or marker file).
- Fixture/test workspaces can be denied permanently via ignore rules/markers.
- External roots cannot be indexed when `deny_external_paths` is enabled.

## Backward compatibility

- Policy file absent → compatibility mode mirrors current behavior but emits a warning recommending an explicit policy.
- Policy file present → it is authoritative.

## Child tickets (implementation order)

1. Policy file parser + in-memory policy object + compat-mode warning.
2. Apply policy in `discover_workspace_scan_roots` (discovery filtering).
3. Scan-root persistence metadata (`source`, `policy_decision`, `workspace_root`) + scan-time enforcement + scan reporting.
4. Query-time final guard tied to policy-allowed roots.
5. CLI/API surfaces for policy management.
6. Regression tests across all enforcement points.

## Audit-roadmap placement

- This tracker now sits directly under the audit-roadmap root and ticket-graph category gates via dependency edges:
  - `edde88d6 depends_on 65d5885b`
  - `1a2b326d depends_on 65d5885b`
- Completion order is therefore enforced: workspace-policy implementation must complete before ticket-graph and root audit closure.
- Purpose: enforce fixture/test workspace exclusion capability before ticket_graph closure so fixture stores are not re-indexed into audit counts.

## Acceptance criteria

- [ ] `.ticket/workspace-policy.toml` is parsed into an in-memory policy object with documented defaults.
- [ ] All three enforcement points (discovery, scan, query) consult the policy.
- [ ] Scan roots persist `source` / `policy_decision` / `workspace_root` metadata for auditability and deterministic rebuilds.
- [ ] CLI commands can show/set policy, add/remove ignore and include rules, and rescan with policy applied.
- [ ] Regression suite proves: child included by default, child ignored via marker, ignored via glob, include override wins, external path denied.
- [ ] Absent-policy compatibility mode preserves current behavior and warns.
- [ ] Spec updated to own this contract with linked evidence before the tracker closes.
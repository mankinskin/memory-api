## Goal

Design and implement `spec-vscode` — a VS Code extension that surfaces the spec store in the sidebar, mirroring the patterns established by `ticket-vscode`. The extension allows developers to browse, search, and advance specifications without leaving the editor.

## Background

`ticket-vscode` (`tools/ticket-vscode/`) provides a proven pattern for VS Code extensions in this repository:
- TypeScript + vscode API
- HTTP client against a backend server (ticket-viewer / spec-viewer)
- TreeDataProvider with grouping, lazy tooltips, and context menus
- Auto-spawns backend binary; port auto-assign; status bar item
- Browser bridge for driving the SPA from the extension

`spec-vscode` should follow the same architecture, substituting `spec-api`/`spec-http`/`spec-viewer` for the ticket equivalents.

## Prerequisites (blocking)

1. **ticket-vscode specification** (must be `reviewed`) — provides the reference pattern
2. **spec-viewer Dioxus SPA** (ticket 06399bb2, must be `done`) — backend to serve the sidebar tree

## Planned Features

| Feature | Notes |
|---|---|
| Activity bar panel | `spec-viewer` viewsContainer, `spec-viewer.specs` TreeView |
| Tree model | Specs grouped by component; collapsible child-spec subtrees |
| Lazy tooltip | `resolveTreeItem` fetches spec body on hover |
| Server lifecycle | Auto-spawns `spec-viewer` binary; port auto-assign; status bar |
| Workspace resolution | Scans `.spec/` dirs in open VS Code folders |
| State commands | advance, set-state, preview, previewDescription |
| CRUD commands | create (slug + component + scope), editTitle, addCodeRef, openBody |
| Search command | Full-text spec search via QuickPick |
| Configuration | serverUrl, workspace, autoRefreshSeconds, autoStartServer, serverBinaryPath |

## Design Phases

### Phase 1 — Specification (this ticket begins here)

Goal: write and approve the full design spec for spec-vscode before any code is written.

- Create spec `spec-vscode` in the spec store (slug: `spec-vscode`)
- Sections: overview, architecture, commands, api-surface, configuration, test-strategy
- Use ticket-vscode spec as reference — note where spec-vscode diverges (e.g. spec state machine, slug validation)
- Add CodeRefs pointing to ticket-vscode source files as cross-references
- Advance spec to `approved`

### Phase 2 — Scaffold

- Copy `tools/ticket-vscode/` to `tools/spec-vscode/`
- Rename package: `spec-viewer`, update `package.json` (displayName, commands, config keys)
- Replace all `ticket-viewer.*` command IDs with `spec-viewer.*`
- Replace `ticketProvider.ts` with `specProvider.ts` (group by component instead of state)
- Replace `api.ts` HTTP calls with spec-viewer API endpoints
- Remove `browserBridge.ts` (not needed for Phase 2; can be re-added later)

### Phase 3 — Commands

Implement these spec-specific commands beyond the base ticket-vscode set:
- `spec-viewer.advanceState` — advances spec through state machine with confirmation
- `spec-viewer.addCodeRef` — 3-step QuickPick: file → symbol type → line range
- `spec-viewer.openBody` — opens `body.md` in the editor
- `spec-viewer.searchSpecs` — full-text search via `GET /api/workspace/:ws/specs?q=`
- `spec-viewer.createSpec` — prompts for title, slug, component, scope

### Phase 4 — Polish and testing

- Status bar item: "Specs: N" badge
- Auto-refresh on `.spec/` file change (fs.watch)
- Unit tests for `buildComponentGroups` tree logic
- Integration test: spawn spec-viewer, assert tree populates

## Acceptance Criteria

- [ ] `spec-vscode` spec created, all sections present, advanced to `approved`
- [ ] Extension activates and shows specs tree in activity bar
- [ ] Specs grouped by component with collapsible subtrees
- [ ] All Phase 3 commands registered and functional
- [ ] `spec-viewer.advanceState` enforces valid state transitions (rejects invalid advances)
- [ ] Status bar shows live spec count
- [ ] Auto-spawns `spec-viewer` binary; survives restart
- [ ] Configuration: all 5 settings work
- [ ] Unit tests pass (`npm test` in `tools/spec-vscode/`)
- [ ] Extension packages to `.vsix` without errors
- [ ] No `set_inner_html` usage — all user content uses text-node APIs

## File Layout (target)

```
tools/spec-vscode/
  package.json          — name: spec-viewer, contributes: commands + config
  tsconfig.json
  jest.config.ts
  src/
    extension.ts        — activation, command registration, server lifecycle
    specProvider.ts     — TreeDataProvider grouped by component
    api.ts              — HTTP client for spec-viewer REST API
  test/unit/
    buildComponentGroups.test.ts
  resources/
    spec.svg
```

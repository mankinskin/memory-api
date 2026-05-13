## Goal

Produce a complete design specification for the existing `ticket-vscode` VS Code extension (`tools/ticket-vscode/`). The spec must be detailed enough to serve as the architectural reference model when designing the new `spec-vscode` extension.

## Context

The `ticket-vscode` extension (package: `ticket-viewer`, v0.1.0) is a fully working VS Code extension that surfaces the ticket graph in the sidebar. It was built before the spec system existed, so it has no formal specification.

A sibling `spec-vscode` extension is planned. Rather than designing spec-vscode from scratch, we should first document ticket-vscode so spec-vscode can follow the same patterns.

## Architecture Overview (to be captured in the spec)

### Source layout

```
tools/ticket-vscode/src/
  extension.ts       — activation, command registration, server lifecycle, status bar
  ticketProvider.ts  — TreeDataProvider (StateGroupItem, TicketItem, DependencyItem)
  api.ts             — typed HTTP client for ticket-viewer REST API
  browserBridge.ts   — Playwright/CDP browser automation bridge
```

### Features

| Feature | Implementation |
|---|---|
| Activity bar panel | Custom viewsContainer + `ticket-viewer.tickets` TreeView |
| Tree model | Tickets grouped by state; collapsible dependency subtrees |
| Lazy tooltip | `resolveTreeItem` fetches description on hover |
| Server lifecycle | Auto-spawns `ticket-viewer` binary; port-auto-assign; status bar item |
| Workspace resolution | Scans `.ticket/` dirs in open VS Code folders; falls back to server API |
| CRUD commands | create, editTitle, setState, editDescription, addDependency, close, cancel, undo, delete |
| Browser bridge | Playwright CDP automation — `bridgeNavigate`, `bridgeConnectCdp`, `bridgeStatus` |
| Configuration | 9 settings: serverUrl, workspace, autoRefreshSeconds, autoStartServer, bridgePort, cdpPort, autoConnectCdp, serverBinaryPath, serverWorkingDirectory |

### Command set (20 commands)

ticket-viewer.openBrowser, refresh, startServer, createTicket, copyId, editTitle, setState, editDescription, previewDescription, closeTicket, cancelTicket, undoTicket, addDependency, deleteTicket, bridgeNavigate, bridgeConnectCdp, bridgeStatus, selectWorkspace, openInTicketViewer, openTicket

### API surface

HTTP client calls to `ticket-viewer` server (default port 3002):
- `GET /api/workspaces`
- `GET /api/workspace/:ws/tickets` (pagination via cursor)
- `GET /api/workspace/:ws/edges?ticket_id=`
- `GET /api/workspace/:ws/schemas`
- `GET /api/workspace/:ws/ticket/:id/description`
- `POST /api/workspace/:ws/tickets` (create)
- `PUT /api/workspace/:ws/ticket/:id` (update title/state)
- `POST /api/workspace/:ws/ticket/:id/close`
- `POST /api/workspace/:ws/ticket/:id/cancel`
- `POST /api/workspace/:ws/ticket/:id/undo`
- `DELETE /api/workspace/:ws/ticket/:id`
- `POST /api/workspace/:ws/edges` (add dependency)

### Browser Bridge

Playwright-based CDP automation. Registers an HTTP control server (bridgePort). Exposes: navigate(url), connectCdp(), getStatus(). Used to drive the ticket-viewer SPA in a browser linked to VS Code's renderer.

## Acceptance Criteria

- [ ] Spec `ticket-vscode` created in the spec store with slug `ticket-vscode`
- [ ] Spec has sections: `overview`, `architecture`, `commands`, `api-surface`, `browser-bridge`, `configuration`, `test-strategy`
- [ ] All 20 commands documented with their purpose, menu placement, and QuickPick/input interactions
- [ ] All 9 configuration settings documented with types, defaults, and effect
- [ ] API surface section covers every HTTP call with method, path, and payload shape
- [ ] Browser Bridge section explains the CDP automation model and security considerations
- [ ] CodeRefs added pointing to `extension.ts`, `ticketProvider.ts`, `api.ts`, `browserBridge.ts`
- [ ] Spec advanced to `reviewed` state
- [ ] `spec health ticket-vscode` reports 0 issues

## Implementation Steps

1. Run `spec create --title 'ticket-vscode Extension' --slug ticket-vscode --component ticket-vscode --scope component`
2. Write body.md: purpose, technology stack (TypeScript, vscode API, Playwright), activation model
3. Add sections: overview, architecture (with ASCII source-layout diagram), commands (table), api-surface (table per endpoint), browser-bridge, configuration (table), test-strategy
4. Add CodeRefs for the 4 source files
5. Advance spec to `reviewed`
6. Run `spec health ticket-vscode` — resolve any issues
7. Update this ticket to `in-review`, then `done`

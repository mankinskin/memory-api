## Source Layout

```
tools/ticket-vscode/
  package.json          — manifest: contributes (views, commands, menus, config), engines, deps
  tsconfig.json         — compiles src/ → out/; strict mode
  jest.config.ts        — Jest + ts-jest; tests in test/unit/
  src/
    extension.ts        — entry point: activate(), deactivate(), all command registrations
    ticketProvider.ts   — TicketTreeProvider (TreeDataProvider + Disposable)
    api.ts              — typed HTTP client for ticket-viewer REST API
    browserBridge.ts    — BrowserBridge: HTTP control server + Playwright/CDP automation
  test/unit/
    buildStateGroups.test.ts
  resources/
    ticket.svg          — activity bar icon
```

## Module Responsibilities

| Module | Responsibility |
|---|---|
| `extension.ts` | VS Code lifecycle: activate/deactivate, server spawn, workspace resolution, status bar, all command handlers |
| `ticketProvider.ts` | Implements `vscode.TreeDataProvider<TreeNode>`; groups tickets by state; manages dependency map; lazy tooltips via `resolveTreeItem`; on-disk folder browsing |
| `api.ts` | Stateless HTTP client functions; all calls carry a 6 s GET timeout / 10 s mutation timeout; cursor-based pagination in `fetchAllTickets` |
| `browserBridge.ts` | Local HTTP control server on a configurable port; VS Code Simple Browser navigation; Playwright-over-CDP page automation (dynamically required) |

## Data Flow

```
VS Code activation
   │
   ├─ startServerTask()   → spawns ticket-viewer --port 0
   │      reads TICKET_VIEWER_PORT=N from stdout
   │
   ├─ resolveActiveWorkspace()
   │      scans .ticket/ dirs → fetchWorkspaces() → QuickPick if ambiguous
   │
   ├─ TicketTreeProvider(serverUrl, workspace, ...)
   │      load():
   │        fetchAllTickets() ─┐
   │        fetchEdges()       ├─ Promise.all → buildStateGroups()
   │        fetchSchemas()    ─┘
   │
   └─ BrowserBridge.start()   → binds HTTP control server on port bridgePort
          _autoConnectCdp()   → probes CDP_PROBE_PORTS [9222,9223,9229,9230]
```

## Tree Node Hierarchy

```
StateGroupItem ("in-implementation (3)")
  └─ TicketItem (root ticket within this state)
       ├─ TicketItem (dep child, same state)
       ├─ TicketFolderItem (sections/)
       │    └─ TicketFileItem (design.md)
       ├─ TicketFileItem (description.md)
       └─ TicketFileItem (ticket.toml)
```

Root tickets within a state group are tickets whose `parentOf` entries contain no other ticket also in the same state (same-state ancestor filter).

## Workspace Resolution Priority

1. `ticketViewer.workspace` setting (explicit) — always wins.
2. Single `.ticket/` folder in open VS Code workspace → server name lookup.
3. Multiple `.ticket/` folders → `workspaceState.get('activeTicketFolder')` → QuickPick.
4. No `.ticket/` folder detected → first workspace name returned by server API.
5. Absolute fallback: `"default"`.

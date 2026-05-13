All 20 commands are prefixed `ticket-viewer.*`. Commands are registered in `extension.ts:activate()`.

## Navigation & Server

| Command ID | Title | Menu | Interaction |
|---|---|---|---|
| `ticket-viewer.openBrowser` | Open Ticket Viewer in Browser | view/title (nav@3) | Calls `simpleBrowser.show(serverUrl)` |
| `ticket-viewer.refresh` | Refresh Tickets | view/title (nav@1) | Calls `provider.refresh()` |
| `ticket-viewer.startServer` | Start Ticket Viewer Server | view/title (nav@2) | Kills existing process; calls `startServerTask()`; re-attaches provider |
| `ticket-viewer.openTicket` | Open Ticket | (command only, invoked by tree item click) | Opens `description.md` as Markdown preview; falls back to SPA URL in Simple Browser |
| `ticket-viewer.openInTicketViewer` | Open in Ticket Viewer | view/item/context (0_open@1) | Opens `serverUrl/workspace/:ws/ticket/:id` in Simple Browser |
| `ticket-viewer.copyId` | Copy Ticket ID | view/item/context (inline) | Writes `ticket.id` to clipboard; shows status bar flash |
| `ticket-viewer.selectWorkspace` | Select Ticket Workspace | (palette) | QuickPick from detected `.ticket/` folders; saves selection to `workspaceState` |
| `ticket-viewer.bridgeStatus` | Browser Bridge: Status | (palette) | Shows `showInformationMessage` with bridge port, CDP status, and current URL |

## Ticket Mutation

| Command ID | Title | Menu | Interaction |
|---|---|---|---|
| `ticket-viewer.createTicket` | New Ticket... | view/title (nav@0) | QuickPick type → InputBox title → `createTicket()` |
| `ticket-viewer.editTitle` | Edit Title... | view/item/context (1_modify@1) | InputBox pre-filled with current title → `updateTicket({ fields: { title } })` |
| `ticket-viewer.setState` | Set State... | view/item/context (1_modify@2) | QuickPick all TICKET_STATES; marks current state → `updateTicket({ state })` |
| `ticket-viewer.editDescription` | Edit Description | view/item/context (1_modify@3) | Opens `description.md` in editor via `vscode.open` |
| `ticket-viewer.previewDescription` | Preview Description | (palette) | Opens `description.md` as Markdown preview to the side |
| `ticket-viewer.addDependency` | Add Dependency... | view/item/context (1_modify@4) | QuickPick from all other tickets → `addEdge(from, to, "depends_on")` |
| `ticket-viewer.closeTicket` | Close (fast-forward to done) | view/item/context (2_lifecycle@1) | Modal confirm → `closeTicket()` |
| `ticket-viewer.cancelTicket` | Cancel Ticket... | view/item/context (2_lifecycle@2) | InputBox for optional reason → `cancelTicket()` |
| `ticket-viewer.undoTicket` | Undo Last Transition | view/item/context (2_lifecycle@3) | Modal confirm → `undoTicket()` |
| `ticket-viewer.deleteTicket` | Delete Ticket... | view/item/context (9_delete@1) | Modal confirm (destructive warning) → `deleteTicket()` |

## Browser Bridge

| Command ID | Title | Menu | Interaction |
|---|---|---|---|
| `ticket-viewer.bridgeNavigate` | Browser Bridge: Open URL | (palette) | InputBox pre-filled with `serverUrl` → `bridge.navigate(url)` |
| `ticket-viewer.bridgeConnectCdp` | Browser Bridge: Connect CDP | (palette) | `bridge.connectCdp()` → success/fail notification |

## TICKET_STATES constant

Hard-coded in `extension.ts` (not loaded from schema at startup):

```ts
const TICKET_STATES = ['new', 'ready', 'in-implementation', 'in-review', 'done', 'cancelled'];
```

The `TicketTreeProvider` loads the live schema state ordering from `GET /api/schema` to sort groups correctly.

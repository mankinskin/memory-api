# ticket-vscode: Replace native tooltip with debounced webview panel beside sidebar

## Problem

The current hover tooltip on ticket tree items appears too quickly (VS Code's default ~500ms) and is positioned by VS Code at the cursor, which can obscure the tree view. Users want a more relaxed hover experience with the detail popup displayed beside the sidebar rather than overlaying it.

## Follow-up to

Ticket `207b70d9` ("ticket-vscode: Show ticket description in hover tooltip") — which implemented the current `resolveTreeItem`-based native tooltip.

## Requirements

1. **Hover delay**: The popup must not appear until the cursor has hovered over a tree item for **2 seconds** continuously.
2. **Popup position**: The detail popup must appear in the **editor area** beside the sidebar (not as a native tooltip overlaying tree items).
3. **Dismiss behavior**: The popup must close when the mouse leaves both the tree item and the popup area (approximated via tree selection changes / focus tracking).
4. **Content**: Same rich markdown content currently shown in the native tooltip (ticket title, ID, state, type, description).

## Technical Constraints

VS Code's TreeView API does not support:
- Custom tooltip delay
- Custom tooltip positioning

Therefore the native tooltip mechanism (`resolveTreeItem` setting `item.tooltip`) must be **replaced** with a custom approach.

## Implementation Plan

### Step 1 — Disable native tooltip

In `ticketProvider.ts`, remove the tooltip assignment from `_setDescriptionTooltip()` and `resolveTreeItem()`. Set `item.tooltip` to a minimal one-liner (e.g. just the short ID) or leave it `undefined` to suppress the native popup.

### Step 2 — Add debounced hover tracking

- Track which `TicketItem` the user is hovering over using the existing `resolveTreeItem` callback (VS Code calls this on hover).
- Implement a **2-second debounce timer**: when `resolveTreeItem` fires, start/restart a timer. If the same item is still being resolved after 2 seconds, trigger the detail panel.
- Cancel the timer if `resolveTreeItem` is called for a different item or if the tree view loses focus.

### Step 3 — Create a webview panel for ticket details

- On timer expiry, open a `vscode.WebviewPanel` in `ViewColumn.Beside` (or `ViewColumn.One`).
- Render the ticket detail content (title, ID, state, type, description) as styled HTML in the webview.
- Reuse a single panel instance — update its content rather than creating a new panel for each hover.
- If the panel already shows the same ticket, skip the update.

### Step 4 — Implement dismiss logic

- Listen for `TreeView.onDidChangeSelection` and `TreeView.onDidChangeVisibility` — when selection changes or tree view hides, close/hide the detail panel.
- Listen for `WebviewPanel.onDidChangeViewState` to detect when the panel loses focus.
- Optionally add a small grace period (e.g. 500ms) before closing to allow the user to move the mouse to the panel without it disappearing.

### Step 5 — Cache and performance

- Reuse the existing `_descriptionCache` for fetched descriptions so the panel can render immediately for previously-seen tickets.
- Pre-fetch descriptions in `resolveTreeItem` (keep the fetch, just don't assign it to the tooltip).

## Files to modify

- `tools/ticket-vscode/src/ticketProvider.ts` — Main changes: hover tracking, debounce, panel lifecycle
- `tools/ticket-vscode/src/extension.ts` — Possibly register panel disposal in extension context

## Acceptance Criteria

- [ ] Hovering over a tree item for less than 2 seconds does NOT show a detail popup.
- [ ] Hovering for 2+ seconds opens a webview panel in the editor area beside the sidebar.
- [ ] Moving the cursor to a different tree item cancels the pending popup and starts a new 2-second timer.
- [ ] The panel content matches the old tooltip content (title, ID, state, type, description).
- [ ] Moving focus away from the tree view or selecting a different item closes the detail panel.
- [ ] Hovering over the same item twice reuses the cached description (no redundant fetch).
- [ ] The native tooltip no longer appears (or only shows a minimal short ID).
# Navigate to ticket URL in Simple Browser on click

## Problem

When a user clicks a ticket in the tree view, the current behavior opens the ticket-viewer root URL and copies the ticket ID to the clipboard. The user must then manually paste and search for the ticket. This is a poor UX — the extension knows which ticket was clicked and the viewer supports hash-based routing to individual tickets.

## Solution

Change the `ticket-viewer.openTicket` command to navigate directly to the ticket's detail URL using the ticket-viewer's hash routing: `{serverUrl}/#/ws/{workspace}/ticket/{ticketId}`.

### Implementation Plan

1. **Update `openTicketViewer()` helper** in `extension.ts`:
   - Change signature to accept an optional ticket ID and workspace.
   - When both are provided, build URL: `${serverUrl}/#/ws/${workspace}/ticket/${ticketId}`.
   - When only serverUrl, open root as before.

2. **Update `ticket-viewer.openTicket` command handler**:
   - Build the ticket-specific URL using `config.serverUrl`, the resolved `workspace`, and `item.ticket.id`.
   - Call `openTicketViewer()` with the full URL.
   - Remove the clipboard copy + info message (no longer needed — the ticket opens directly).

3. **Keep `ticket-viewer.openBrowser` unchanged**:
   - This command opens the root viewer and should remain as-is for general browsing.

### Files to Modify

- `tools/ticket-vscode/src/extension.ts` — update `openTicketViewer()` and `openTicket` command handler

## Acceptance Criteria

- [ ] Clicking a ticket in the tree view opens Simple Browser at `{serverUrl}/#/ws/{workspace}/ticket/{ticketId}`
- [ ] The ticket detail view loads correctly in the embedded browser
- [ ] The "Open Ticket Viewer in Browser" toolbar button still opens the root URL
- [ ] No clipboard copy or info message when clicking a ticket (direct navigation replaces it)
- [ ] Extension compiles without errors (`npm run compile`)

## Risk

Low — changes only the URL passed to `simpleBrowser.show`. No new API calls or dependencies.
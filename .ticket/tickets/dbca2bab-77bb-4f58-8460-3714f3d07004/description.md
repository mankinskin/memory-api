# Auto-start ticket-viewer server on extension activation

## Problem

Currently, users must manually click the ▶ button in the Tickets sidebar or run the "Start Ticket Viewer Server" command before the tree view can display tickets. If the server is not running, the tree view shows a "Server not reachable" error on every refresh cycle.

## Solution

Automatically invoke the `ticket-viewer: start` VS Code task when the extension activates (`onStartupFinished`), so the server is ready by the time the user opens the sidebar.

### Implementation Plan

1. **Add `ticketViewer.autoStartServer` setting** in `package.json` → `contributes.configuration.properties`:
   - Type: `boolean`
   - Default: `true`
   - Description: "Automatically start the ticket-viewer server when the extension activates."

2. **Read setting in `activate()`** (`extension.ts`):
   - Add `autoStartServer` to the `readConfig()` return.
   - If `config.autoStartServer` is `true`, call `startServerTask()` early in `activate()`.

3. **Handle task-not-found gracefully**:
   - The existing `startServerTask()` already catches errors and shows a message.
   - Add a short initial delay (1-2 seconds) before first refresh attempt to give the server time to bind.

4. **React to config changes**:
   - In the `onDidChangeConfiguration` handler, do NOT re-start the server if the setting changes to `true` mid-session (server start is a one-time activation action). Just update the stored config.

### Files to Modify

- `tools/ticket-vscode/package.json` — add new configuration property
- `tools/ticket-vscode/src/extension.ts` — read config, call startServerTask() on activation

## Acceptance Criteria

- [ ] New `ticketViewer.autoStartServer` boolean setting exists (default `true`)
- [ ] When `autoStartServer` is `true`, the `ticket-viewer: start` task is invoked automatically on extension activation
- [ ] When `autoStartServer` is `false`, no automatic server start occurs (existing manual workflow)
- [ ] If the task definition is missing from `.vscode/tasks.json`, a helpful error message is shown
- [ ] Extension compiles without errors (`npm run compile`)

## Risk

Low — adds a single conditional call to an existing function during activation. No API changes.
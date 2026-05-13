# Auto-detect .ticket workspace from open VS Code folders

## Problem

The ticket-vscode extension currently resolves the ticket workspace through a hardcoded chain:

1. Read `ticketViewer.workspace` setting (default: `"default"`)
2. If empty, call `GET /api/workspaces` and pick the first result
3. Fall back to `"default"`

This means **every VS Code window uses the same ticket workspace**, regardless of which project folder is open. The server itself discovers workspaces from `~/.ticket-workspaces.toml` — a global config file with no awareness of which VS Code window is asking.

**Concrete pain points:**
- Opening two VS Code windows on different projects (each with its own `.ticket/` directory) shows the same tickets in both
- Users must manually edit `ticketViewer.workspace` per-window to switch
- In a multi-root workspace with multiple folders each containing `.ticket/`, there is no way to select which one to view

## Solution

Make the extension workspace-aware by scanning `vscode.workspace.workspaceFolders` for `.ticket/` directories and automatically selecting the right one. When multiple are found, show a QuickPick selector.

### Implementation Plan

#### 1. Add `.ticket/` directory scanner

Create a new function in `extension.ts` (or a small `workspaceDetect.ts` module):

```typescript
import * as vscode from "vscode";
import * as fs from "node:fs";
import * as path from "node:path";

interface DetectedWorkspace {
  /** Display name derived from folder name */
  name: string;
  /** Absolute path to the .ticket/ directory */
  ticketPath: string;
  /** The VS Code workspace folder it belongs to */
  folder: vscode.WorkspaceFolder;
}

function detectTicketWorkspaces(): DetectedWorkspace[] {
  const folders = vscode.workspace.workspaceFolders ?? [];
  const results: DetectedWorkspace[] = [];
  for (const folder of folders) {
    const ticketDir = path.join(folder.uri.fsPath, ".ticket");
    if (fs.existsSync(ticketDir) && fs.statSync(ticketDir).isDirectory()) {
      results.push({
        name: folder.name,
        ticketPath: ticketDir,
        folder,
      });
    }
  }
  return results;
}
```

#### 2. Update workspace resolution flow

Replace the current `resolveWorkspace()` with a multi-step strategy:

1. **Explicit config wins**: If `ticketViewer.workspace` is set to a non-empty, non-`"default"` value, use it (existing behavior — user override).
2. **Scan VS Code folders**: Call `detectTicketWorkspaces()`.
   - **0 found**: Fall back to server API (`/api/workspaces`) → first result → `"default"` (current behavior).
   - **1 found**: Use that workspace automatically. The workspace `name` must match a name registered with the server (see step 3).
   - **Multiple found**: Show a `vscode.window.showQuickPick()` letting the user choose. Store the selection in workspace-scoped settings so it persists per window. Show a status bar indicator of the active workspace.
3. **Ensure server knows the workspace**: The detected `.ticket/` path must be registered in the server's `WorkspaceRegistry`. Two options:
   - **Option A (recommended)**: Pass `--index-root <detected-path>` when auto-starting the server task, so the server opens the right store. Requires the auto-start ticket (`dbca2bab`) to be implemented first.
   - **Option B**: Rely on `~/.ticket-workspaces.toml` already having the workspace registered. If not found, show a warning with a "Register" action that appends the entry.

#### 3. Server task integration (depends on auto-start ticket)

Update `startServerTask()` to pass the detected workspace path:

```typescript
// Instead of a fixed task name, build a shell command with the detected path
async function startServerTask(indexRoot?: string): Promise<void> {
  if (indexRoot) {
    // Create a dynamic terminal task with --index-root
    const terminal = vscode.window.createTerminal({
      name: "ticket-viewer",
      shellPath: "cargo",
      shellArgs: ["run", "-p", "ticket-viewer", "--", "--index-root", indexRoot],
    });
    terminal.show(true);
  } else {
    // Fall back to the predefined task
    await vscode.commands.executeCommand("workbench.action.tasks.runTask", "ticket-viewer: start");
  }
}
```

#### 4. Multi-root workspace QuickPick

When multiple `.ticket/` directories are found:

```typescript
async function pickWorkspace(detected: DetectedWorkspace[]): Promise<DetectedWorkspace | undefined> {
  const items = detected.map(d => ({
    label: d.name,
    description: d.ticketPath,
    detail: `Folder: ${d.folder.uri.fsPath}`,
    workspace: d,
  }));

  const pick = await vscode.window.showQuickPick(items, {
    placeHolder: "Multiple .ticket workspaces found — select one",
    title: "Ticket Workspace",
  });

  return pick?.workspace;
}
```

#### 5. React to workspace folder changes

Register a `vscode.workspace.onDidChangeWorkspaceFolders` listener to re-scan when folders are added/removed:

```typescript
context.subscriptions.push(
  vscode.workspace.onDidChangeWorkspaceFolders(() => {
    // Re-detect and potentially re-prompt if the active workspace was removed
    // or a new .ticket/ appeared
  }),
);
```

#### 6. Status bar workspace indicator

Update the existing status bar item to show which ticket workspace is active, e.g. `$(issues) context-engine: 3 open, 1 in-progress`. This helps disambiguate in multi-root setups.

### Files to Modify

- `tools/ticket-vscode/package.json` — update `ticketViewer.workspace` description to explain auto-detection
- `tools/ticket-vscode/src/extension.ts` — new detection logic, QuickPick, folder-change listener, server task integration
- `tools/ticket-vscode/src/api.ts` — no changes expected (server API calls remain the same)

### Edge Cases

- **No `.ticket/` in any open folder**: Fall back to current server-driven resolution (transparent)
- **Folder with `.ticket/` removed from workspace**: Re-scan, switch to next available or prompt
- **Server not running + auto-detect**: Works with auto-start ticket — start server pointed at detected path
- **User overrides `ticketViewer.workspace` in settings**: Always wins over auto-detect (explicit is better)
- **`.ticket-workspace` file in project root**: The server/CLI already supports this for their own resolution; the extension should respect the same convention if we skip folder scanning

## Acceptance Criteria

- [ ] Extension scans `vscode.workspace.workspaceFolders` for `.ticket/` directories on activation
- [ ] Single `.ticket/` found → automatically selected without user interaction
- [ ] Multiple `.ticket/` found → QuickPick dropdown shown, selection persisted per window
- [ ] No `.ticket/` found → falls back to server API resolution (existing behavior)
- [ ] Explicit `ticketViewer.workspace` setting overrides auto-detection
- [ ] Status bar shows which ticket workspace is active
- [ ] Workspace re-scanned when folders are added/removed from VS Code workspace
- [ ] Extension compiles without errors (`npm run compile`)

## Dependencies

- Soft dependency on `dbca2bab` (auto-start server) for option A of server task integration
- Linked to `576c5f77` (open ticket URL) and `207b70d9` (hover description) as sibling improvements

## Risk

Medium — changes the core workspace resolution logic that all other features depend on. Must preserve backwards compatibility when no `.ticket/` directories are found.
import * as vscode from 'vscode';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { TicketTreeProvider, TicketItem } from './ticketProvider';
import { fetchWorkspaces } from './api';
import { BrowserBridge } from './browserBridge';

function readConfig() {
  const cfg = vscode.workspace.getConfiguration('ticketViewer');
  return {
    serverUrl: cfg.get<string>('serverUrl', 'http://localhost:3002'),
    workspace: cfg.get<string>('workspace', ''),
    autoRefreshSeconds: cfg.get<number>('autoRefreshSeconds', 30),
    autoStartServer: cfg.get<boolean>('autoStartServer', true),
    bridgePort: cfg.get<number>('bridgePort', 0),
    cdpPort: cfg.get<number>('cdpPort', 0),
    autoConnectCdp: cfg.get<boolean>('autoConnectCdp', true),
  };
}

interface DetectedWorkspace {
  folderName: string;
  ticketPath: string;
  folder: vscode.WorkspaceFolder;
}

function detectTicketWorkspaces(): DetectedWorkspace[] {
  const folders = vscode.workspace.workspaceFolders ?? [];
  return folders.flatMap(folder => {
    const ticketDir = path.join(folder.uri.fsPath, '.ticket');
    try {
      if (fs.statSync(ticketDir).isDirectory()) {
        return [{ folderName: folder.name, ticketPath: ticketDir, folder }];
      }
    } catch { /* directory not found */ }
    return [];
  });
}

/** Resolve the active workspace name and a display label for the status bar. */
async function resolveActiveWorkspace(
  serverUrl: string,
  configured: string,
  context: vscode.ExtensionContext,
): Promise<{ workspace: string; displayName: string }> {
  // Explicit user config always wins.
  if (configured.trim() !== '') {
    return { workspace: configured.trim(), displayName: configured.trim() };
  }

  // Scan open VS Code folders for .ticket/ directories.
  const detected = detectTicketWorkspaces();

  // Fetch workspace names the server currently knows.
  let serverWorkspaces: string[] = [];
  try {
    const list = await fetchWorkspaces(serverUrl);
    serverWorkspaces = list.map(w => w.name);
  } catch { /* server may not be running yet */ }

  if (detected.length === 1) {
    const { folderName } = detected[0];
    const wsName = serverWorkspaces.includes(folderName)
      ? folderName
      : (serverWorkspaces[0] ?? 'default');
    return { workspace: wsName, displayName: folderName };
  }

  if (detected.length > 1) {
    // Restore a previously chosen folder for this window.
    const stored = context.workspaceState.get<string>('activeTicketFolder');
    if (stored) {
      const match = detected.find(d => d.folderName === stored);
      if (match) {
        const wsName = serverWorkspaces.includes(match.folderName)
          ? match.folderName
          : (serverWorkspaces[0] ?? 'default');
        return { workspace: wsName, displayName: match.folderName };
      }
    }

    // Multiple candidates — ask the user.
    const items = detected.map(d => ({
      label: d.folderName,
      description: d.ticketPath,
      folderName: d.folderName,
    }));
    const pick = await vscode.window.showQuickPick(items, {
      placeHolder: 'Multiple .ticket workspaces found — select one',
      title: 'Active Ticket Workspace',
    });
    if (pick) {
      await context.workspaceState.update('activeTicketFolder', pick.folderName);
      const wsName = serverWorkspaces.includes(pick.folderName)
        ? pick.folderName
        : (serverWorkspaces[0] ?? 'default');
      return { workspace: wsName, displayName: pick.folderName };
    }
  }

  // Fallback: first workspace from server.
  const ws = serverWorkspaces[0] ?? 'default';
  return { workspace: ws, displayName: ws };
}

function openTicketViewer(url: string): void {
  void vscode.commands.executeCommand('simpleBrowser.show', url);
}

async function startServerTask(): Promise<void> {
  // Invoke the ticket-viewer: start task defined in .vscode/tasks.json.
  try {
    await vscode.commands.executeCommand(
      'workbench.action.tasks.runTask',
      'ticket-viewer: start',
    );
  } catch {
    void vscode.window.showErrorMessage(
      'Could not start "ticket-viewer: start" task. Make sure .vscode/tasks.json is configured.',
    );
  }
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  let config = readConfig();
  let { workspace, displayName } = await resolveActiveWorkspace(
    config.serverUrl,
    config.workspace,
    context,
  );

  // ── Auto-start server ────────────────────────────────────────────
  if (config.autoStartServer) {
    void startServerTask();
  }

  // ── Tree data provider ──────────────────────────────────────────
  const provider = new TicketTreeProvider(
    config.serverUrl,
    workspace,
    config.autoRefreshSeconds,
  );
  context.subscriptions.push(provider);

  // If we auto-started the server give it a moment then retry.
  if (config.autoStartServer) {
    setTimeout(() => provider.refresh(), 3000);
  }

  const treeView = vscode.window.createTreeView('ticket-viewer.tickets', {
    treeDataProvider: provider,
    showCollapseAll: true,
  });
  context.subscriptions.push(treeView);

  // ── Status bar item ───────────────────────────────────────────────────────
  const statusBarItem = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    100,
  );
  statusBarItem.command = 'ticket-viewer.openBrowser';
  statusBarItem.tooltip = `Open Ticket Viewer (${config.serverUrl})`;
  statusBarItem.show();
  context.subscriptions.push(statusBarItem);

  function updateStatusBar(): void {
    const tickets = provider.allTickets;
    const openCount = tickets.filter(t => t.state === 'open').length;
    const inProgressCount = tickets.filter(t => t.state === 'in-progress').length;
    const prefix = `$(issues) ${displayName}`;

    if (tickets.length === 0) {
      statusBarItem.text = prefix;
    } else {
      const parts: string[] = [];
      if (openCount > 0) { parts.push(`${openCount} open`); }
      if (inProgressCount > 0) { parts.push(`${inProgressCount} in-progress`); }
      statusBarItem.text = parts.length > 0
        ? `${prefix}: ${parts.join(', ')}`
        : `${prefix} (${tickets.length})`;
    }
  }

  // Update status bar whenever the tree data changes.
  context.subscriptions.push(
    provider.onDidChangeTreeData(() => updateStatusBar()),
  );

  // ── Commands ──────────────────────────────────────────────────────────────
  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.openBrowser', () => {
      openTicketViewer(config.serverUrl);
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.refresh', () => {
      provider.refresh();
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.startServer', async () => {
      await startServerTask();
      // Give the server a moment to start, then refresh.
      setTimeout(() => provider.refresh(), 3000);
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      'ticket-viewer.openTicket',
      (item: TicketItem) => {
        const ticketUrl = `${config.serverUrl}/#/ws/${encodeURIComponent(workspace)}/ticket/${encodeURIComponent(item.ticket.id)}`;
        openTicketViewer(ticketUrl);
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      'ticket-viewer.copyId',
      (item: TicketItem) => {
        void vscode.env.clipboard.writeText(item.ticket.id).then(() => {
          void vscode.window.showInformationMessage(
            `Copied: ${item.ticket.id}`,
          );
        });
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.selectWorkspace', async () => {
      const detected = detectTicketWorkspaces();
      if (detected.length === 0) {
        void vscode.window.showInformationMessage(
          'No .ticket workspaces found in the currently open folders.',
        );
        return;
      }
      const items = detected.map(d => ({
        label: d.folderName,
        description: d.ticketPath,
        folderName: d.folderName,
      }));
      const pick = await vscode.window.showQuickPick(items, {
        placeHolder: 'Select active ticket workspace',
        title: 'Active Ticket Workspace',
      });
      if (!pick) { return; }
      await context.workspaceState.update('activeTicketFolder', pick.folderName);
      const resolved = await resolveActiveWorkspace(config.serverUrl, config.workspace, context);
      workspace = resolved.workspace;
      displayName = resolved.displayName;
      provider.update(config.serverUrl, workspace, config.autoRefreshSeconds);
      updateStatusBar();
    }),
  );

  // ── Browser Bridge ──────────────────────────────────────────────────────────
  const bridge = new BrowserBridge({
    controlPort: config.bridgePort,
    cdpPort: config.cdpPort,
    autoConnectCdp: config.autoConnectCdp,
  });
  context.subscriptions.push(bridge);

  // Start the control server immediately.
  bridge.start().catch(err => {
    const msg = err instanceof Error ? err.message : String(err);
    void vscode.window.showWarningMessage(`Browser Bridge failed to start: ${msg}`);
  });

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.bridgeNavigate', async () => {
      const url = await vscode.window.showInputBox({
        prompt: 'URL to open in Simple Browser',
        value: config.serverUrl,
      });
      if (url) { await bridge.navigate(url); }
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.bridgeConnectCdp', async () => {
      const ok = await bridge.connectCdp();
      if (ok) {
        void vscode.window.showInformationMessage('Browser Bridge: CDP connected.');
      }
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.bridgeStatus', () => {
      const s = bridge.state;
      void vscode.window.showInformationMessage(
        `Bridge port: ${s.controlPort} | CDP: ${s.cdpConnected ? 'connected' : 'disconnected'} | URL: ${s.currentUrl ?? 'none'}`
      );
    }),
  );

  // ── React to VS Code folder changes ───────────────────────────────
  context.subscriptions.push(
    vscode.workspace.onDidChangeWorkspaceFolders(async () => {
      const resolved = await resolveActiveWorkspace(config.serverUrl, config.workspace, context);
      workspace = resolved.workspace;
      displayName = resolved.displayName;
      provider.update(config.serverUrl, workspace, config.autoRefreshSeconds);
      updateStatusBar();
    }),
  );

  // ── React to config changes ───────────────────────────────────────────
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(async e => {
      if (!e.affectsConfiguration('ticketViewer')) { return; }
      config = readConfig();
      const resolved = await resolveActiveWorkspace(config.serverUrl, config.workspace, context);
      workspace = resolved.workspace;
      displayName = resolved.displayName;
      provider.update(config.serverUrl, workspace, config.autoRefreshSeconds);
      statusBarItem.tooltip = `Open Ticket Viewer (${config.serverUrl})`;
      updateStatusBar();
    }),
  );
}

export function deactivate(): void {
  // Nothing to clean up beyond subscriptions (handled by context.subscriptions).
}

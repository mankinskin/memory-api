import * as vscode from 'vscode';
import { TicketTreeProvider, TicketItem } from './ticketProvider';
import { fetchWorkspaces } from './api';
import { BrowserBridge } from './browserBridge';

function readConfig() {
  const cfg = vscode.workspace.getConfiguration('ticketViewer');
  return {
    serverUrl: cfg.get<string>('serverUrl', 'http://localhost:3002'),
    workspace: cfg.get<string>('workspace', 'default'),
    autoRefreshSeconds: cfg.get<number>('autoRefreshSeconds', 30),
    bridgePort: cfg.get<number>('bridgePort', 0),
    cdpPort: cfg.get<number>('cdpPort', 0),
    autoConnectCdp: cfg.get<boolean>('autoConnectCdp', true),
  };
}

/** Resolve the workspace name: use config value if set, otherwise first available. */
async function resolveWorkspace(serverUrl: string, configured: string): Promise<string> {
  if (configured.trim() !== '') { return configured.trim(); }
  try {
    const workspaces = await fetchWorkspaces(serverUrl);
    return workspaces[0]?.name ?? 'default';
  } catch {
    return 'default';
  }
}

function openTicketViewer(serverUrl: string): void {
  void vscode.commands.executeCommand('simpleBrowser.show', serverUrl);
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
  const workspace = await resolveWorkspace(config.serverUrl, config.workspace);

  // ── Tree data provider ────────────────────────────────────────────────────
  const provider = new TicketTreeProvider(
    config.serverUrl,
    workspace,
    config.autoRefreshSeconds,
  );
  context.subscriptions.push(provider);

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

    if (tickets.length === 0) {
      statusBarItem.text = '$(issues) Tickets';
    } else {
      const parts: string[] = [];
      if (openCount > 0) { parts.push(`${openCount} open`); }
      if (inProgressCount > 0) { parts.push(`${inProgressCount} in-progress`); }
      statusBarItem.text = parts.length > 0
        ? `$(issues) ${parts.join(', ')}`
        : `$(issues) ${tickets.length} tickets`;
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
        // Open the viewer and copy the ticket ID so the user can search for it.
        openTicketViewer(config.serverUrl);
        void vscode.env.clipboard.writeText(item.ticket.id).then(() => {
          void vscode.window.showInformationMessage(
            `Ticket ID copied to clipboard: ${item.ticket.id.slice(0, 8)}…`,
          );
        });
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

  // ── React to config changes ───────────────────────────────────────────────
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(async e => {
      if (!e.affectsConfiguration('ticketViewer')) { return; }
      config = readConfig();
      const ws = await resolveWorkspace(config.serverUrl, config.workspace);
      provider.update(config.serverUrl, ws, config.autoRefreshSeconds);
      statusBarItem.tooltip = `Open Ticket Viewer (${config.serverUrl})`;
    }),
  );
}

export function deactivate(): void {
  // Nothing to clean up beyond subscriptions (handled by context.subscriptions).
}

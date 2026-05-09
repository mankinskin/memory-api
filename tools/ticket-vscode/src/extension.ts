import * as vscode from 'vscode';
import { TicketTreeProvider } from './ticketProvider';
import { registerExtensionCommands, type ActivationState } from './extensionCommands';
import {
  pingServer,
  pollUntilReachable,
  readConfig,
  resolveActiveWorkspace,
  resolveTicketsDir,
  startServerTask,
} from './extensionSupport';

let _serverProcess: import('node:child_process').ChildProcess | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const outputChannel = vscode.window.createOutputChannel('Ticket Viewer Server');
  context.subscriptions.push(outputChannel);

  const state: ActivationState = {
    config: readConfig(),
    serverUrl: '',
    workspace: '',
    displayName: '',
  };
  state.serverUrl = state.config.serverUrl;

  if (state.config.autoStartServer) {
    if (await pingServer(state.config.serverUrl)) {
      outputChannel.appendLine(`[ticket-viewer] Existing server detected at ${state.config.serverUrl} — skipping auto-start.`);
      state.serverUrl = state.config.serverUrl;
    } else {
      try {
        const handle = await startServerTask(outputChannel, state.config);
        _serverProcess = handle.process;
        state.serverUrl = handle.serverUrl;
        vscode.window.setStatusBarMessage(`$(server) Ticket server running on ${state.serverUrl}`, 5000);
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        outputChannel.appendLine(`[ticket-viewer] Failed to start server: ${msg}`);
        void vscode.window.showWarningMessage(`Ticket Viewer server failed to start: ${msg}`);
      }
    }
  }

  const resolved = await resolveActiveWorkspace(
    state.serverUrl,
    state.config.workspace,
    context,
  );
  state.workspace = resolved.workspace;
  state.displayName = resolved.displayName;

  const provider = new TicketTreeProvider(
    state.serverUrl,
    state.workspace,
    state.config.autoRefreshSeconds,
    resolveTicketsDir(state.workspace),
  );
  context.subscriptions.push(provider);

  if (state.config.autoStartServer && _serverProcess) {
    void pollUntilReachable(state.serverUrl, 30_000).then(() => provider.refresh());
  }

  const treeView = vscode.window.createTreeView('ticket-viewer.tickets', {
    treeDataProvider: provider,
    showCollapseAll: true,
  });
  context.subscriptions.push(treeView);

  const statusBarItem = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    100,
  );
  statusBarItem.command = 'ticket-viewer.openBrowser';
  statusBarItem.tooltip = `Open Ticket Viewer (${state.serverUrl})`;
  statusBarItem.show();
  context.subscriptions.push(statusBarItem);

  function updateStatusBar(): void {
    const tickets = provider.allTickets;
    const newCount = tickets.filter(t => t.state === 'new').length;
    const inImplCount = tickets.filter(t => t.state === 'in-implementation').length;
    const prefix = `$(issues) ${state.displayName}`;

    if (tickets.length === 0) {
      statusBarItem.text = prefix;
    } else {
      const parts: string[] = [];
      if (newCount > 0) { parts.push(`${newCount} new`); }
      if (inImplCount > 0) { parts.push(`${inImplCount} in-impl`); }
      statusBarItem.text = parts.length > 0
        ? `${prefix}: ${parts.join(', ')}`
        : `${prefix} (${tickets.length})`;
    }
  }

  // Update status bar whenever the tree data changes.
  context.subscriptions.push(
    provider.onDidChangeTreeData(() => updateStatusBar()),
  );

  registerExtensionCommands({
    context,
    state,
    provider,
    outputChannel,
    statusBarItem,
    updateStatusBar,
    getServerProcess: () => _serverProcess,
    setServerProcess: process => {
      _serverProcess = process;
    },
  });
}

export function deactivate(): void {
  // Kill the background server process if we started it.
  if (_serverProcess && !_serverProcess.killed) {
    _serverProcess.kill();
    _serverProcess = undefined;
  }
}

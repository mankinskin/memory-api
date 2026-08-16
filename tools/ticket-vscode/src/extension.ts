import * as vscode from 'vscode';
import { TicketTreeProvider } from './ticketProvider';
import { registerExtensionCommands, type ActivationState } from './extensionCommands';
import { registerTerminalObserverCommand } from './terminalObserver';
import { initWasmCore, type CoreApi } from './coreLoader';
import { detectHostKind } from './hostCapabilities';
import {
  discoverRunningServerUrl,
  pingServer,
  pollUntilReachable,
  readConfig,
  rememberServerUrl,
  resolveActiveWorkspace,
  resolveTicketsDir,
  resolveTicketsDirUri,
  startServerTask,
} from './extensionSupport';

let _serverProcess: import('node:child_process').ChildProcess | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const outputChannel = vscode.window.createOutputChannel('Ticket Viewer Server');
  context.subscriptions.push(outputChannel);

  // ── Load WASM core ─────────────────────────────────────────────────────────
  let core: CoreApi | undefined;
  try {
    const wasmUri = vscode.Uri.joinPath(
      context.extensionUri, 'pkg', 'ticket_vscode_core_bg.wasm',
    );
    const wasmBytes = await vscode.workspace.fs.readFile(wasmUri);
    core = await initWasmCore(wasmBytes);
    outputChannel.appendLine(
      `[ticket-viewer] WASM core loaded — version ${core.core_version()}`,
    );
  } catch (err) {
    outputChannel.appendLine(
      `[ticket-viewer] WASM core failed to load: ${String(err)} — degraded mode active`,
    );
  }

  // ── Host-kind detection and capability gating ──────────────────────────────
  const hostKind = detectHostKind({
    uiKind: vscode.env.uiKind,
    extensionKind: context.extension?.extensionKind ?? vscode.ExtensionKind.Workspace,
    remoteName: vscode.env.remoteName,
    isVirtualWorkspace: vscode.workspace.workspaceFolders
      ? vscode.workspace.workspaceFolders.some(f => f.uri.scheme !== 'file')
      : false,
    isTrusted: vscode.workspace.isTrusted,
  });
  const serverControlEnabled = core ? core.supports_server_control(hostKind) : false;
  const browserBridgeEnabled = core ? core.supports_browser_bridge(hostKind) : false;

  void vscode.commands.executeCommand(
    'setContext', 'ticketViewer.serverControlEnabled', serverControlEnabled,
  );
  void vscode.commands.executeCommand(
    'setContext', 'ticketViewer.browserBridgeEnabled', browserBridgeEnabled,
  );

  const state: ActivationState = {
    config: readConfig(),
    serverUrl: '',
    workspace: '',
    displayName: '',
  };
  state.serverUrl = state.config.serverUrl;

  const discoveredServerUrl = await discoverRunningServerUrl(context, state.config, outputChannel);
  if (discoveredServerUrl) {
    state.serverUrl = discoveredServerUrl;
  }

  if (!discoveredServerUrl && state.config.autoStartServer) {
    if (await pingServer(state.config.serverUrl)) {
      outputChannel.appendLine(`[ticket-viewer] Existing server detected at ${state.config.serverUrl} — skipping auto-start.`);
      state.serverUrl = state.config.serverUrl;
      await rememberServerUrl(context, state.serverUrl);
    } else {
      try {
        const handle = await startServerTask(outputChannel, state.config);
        _serverProcess = handle.process;
        state.serverUrl = handle.serverUrl;
        await rememberServerUrl(context, state.serverUrl);
        vscode.window.setStatusBarMessage(`$(server) Ticket server running on ${state.serverUrl}`, 5000);
      } catch (err) {
        const fallbackServerUrl = await discoverRunningServerUrl(
          context,
          state.config,
          outputChannel,
          [state.serverUrl],
        );
        if (fallbackServerUrl) {
          state.serverUrl = fallbackServerUrl;
          vscode.window.setStatusBarMessage(`$(server) Connected to running ticket server on ${state.serverUrl}`, 5000);
        } else {
          const msg = err instanceof Error ? err.message : String(err);
          outputChannel.appendLine(`[ticket-viewer] Failed to start server: ${msg}`);
          void vscode.window.showWarningMessage(`Ticket Viewer server failed to start: ${msg}`);
        }
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
    resolveTicketsDirUri(state.workspace, state.displayName),
    async () => {
      const recoveredServerUrl = await discoverRunningServerUrl(
        context,
        state.config,
        outputChannel,
        [state.serverUrl],
      );
      if (!recoveredServerUrl) {
        return undefined;
      }

      state.serverUrl = recoveredServerUrl;
      await rememberServerUrl(context, state.serverUrl);
      const resolvedWorkspace = await resolveActiveWorkspace(
        state.serverUrl,
        state.config.workspace,
        context,
      );
      state.workspace = resolvedWorkspace.workspace;
      state.displayName = resolvedWorkspace.displayName;
      statusBarItem.tooltip = `Open Ticket Viewer (${state.serverUrl})`;
      updateStatusBar();

      return {
        baseUrl: state.serverUrl,
        workspace: state.workspace,
        ticketsDirUri: resolveTicketsDirUri(state.workspace, state.displayName),
      };
    },
    undefined, // workspaceFs — set up separately for desktop host
    core,
  );
  context.subscriptions.push(provider);

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
    const newCount = tickets.filter(t => t.state === 'open').length;
    const inImplCount = tickets.filter(t => t.state === 'in-implementation').length;
    const prefix = `$(issues) ${state.displayName}`;
    const filterSummary = provider.filterSummary;

    treeView.message = filterSummary ? `Filters: ${filterSummary}` : undefined;

    if (tickets.length === 0) {
      statusBarItem.text = prefix;
    } else {
      const parts: string[] = [];
      if (newCount > 0) { parts.push(`${newCount} open`); }
      if (inImplCount > 0) { parts.push(`${inImplCount} in-impl`); }
      statusBarItem.text = parts.length > 0
        ? `${prefix}: ${parts.join(', ')}`
        : `${prefix} (${tickets.length})`;
    }
  }

  async function syncProviderToResolvedWorkspace(): Promise<void> {
    const resolvedWorkspace = await resolveActiveWorkspace(
      state.serverUrl,
      state.config.workspace,
      context,
    );
    state.workspace = resolvedWorkspace.workspace;
    state.displayName = resolvedWorkspace.displayName;
    provider.update(
      state.serverUrl,
      state.workspace,
      state.config.autoRefreshSeconds,
      resolveTicketsDirUri(state.workspace, state.displayName),
    );
    statusBarItem.tooltip = `Open Ticket Viewer (${state.serverUrl})`;
    updateStatusBar();
  }

  // Update status bar whenever the tree data changes.
  context.subscriptions.push(
    provider.onDidChangeTreeData(() => updateStatusBar()),
  );

  if (state.config.autoStartServer && _serverProcess) {
    void pollUntilReachable(state.serverUrl, 30_000)
      .then(() => syncProviderToResolvedWorkspace())
      .catch(() => undefined);
  }

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
    core,
  });
  registerTerminalObserverCommand(context, outputChannel);
}

export function deactivate(): void {
  // Kill the background server process if we started it.
  if (_serverProcess && !_serverProcess.killed) {
    _serverProcess.kill();
    _serverProcess = undefined;
  }
}

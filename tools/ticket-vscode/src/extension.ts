import * as vscode from 'vscode';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { spawn, type ChildProcess } from 'node:child_process';
import { TicketTreeProvider, TicketItem } from './ticketProvider';
import {
  fetchWorkspaces,
  createTicket, updateTicket, closeTicket, cancelTicket, undoTicket, deleteTicket, addEdge,
} from './api';
import { BrowserBridge } from './browserBridge';

// All known states in progression order.
const TICKET_STATES = [
  'new', 'ready', 'in-implementation',
  'in-review', 'done', 'cancelled',
];

// Known ticket types (offered as QuickPick defaults).
const TICKET_TYPES = ['tracker-improvement'];

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
    serverBinaryPath: cfg.get<string>('serverBinaryPath', ''),
    serverWorkingDirectory: cfg.get<string>('serverWorkingDirectory', ''),
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

/** Derive the absolute path to .ticket/tickets/ for the active workspace name, if detectable. */
function resolveTicketsDir(wsName: string): string | undefined {
  const detected = detectTicketWorkspaces();
  const match = detected.find(d => d.folderName === wsName) ?? detected[0];
  if (!match) { return undefined; }
  const dir = path.join(match.ticketPath, 'tickets');
  try { if (fs.statSync(dir).isDirectory()) { return dir; } } catch { /* not found */ }
  return undefined;
}

/** Resolve the binary, args, and working-directory for the server process. */
function resolveServerLaunch(config: ReturnType<typeof readConfig>): {
  cmd: string;
  args: string[];
  cwd: string | undefined;
} {
  const detected = detectTicketWorkspaces();

  // Working directory: explicit config wins, then first .ticket workspace, then any workspace folder.
  const cwd = config.serverWorkingDirectory.trim() !== ''
    ? config.serverWorkingDirectory.trim()
    : (detected[0]?.folder.uri.fsPath
        ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath);

  // Always pass --index-root so the server opens the correct ticket store
  // regardless of the server's workspace resolution chain (which would otherwise
  // walk up from cwd, check ~/.ticket-workspaces.toml, or fall back to ~/.ticket-index/).
  //
  // Priority:
  //   1. Detected .ticket/ directory in the open VS Code workspace folder.
  //   2. The cwd itself (covers the serverWorkingDirectory override case).
  //      The server's own resolution chain handles the rest from there.
  const indexRoot = detected[0]?.ticketPath ?? undefined;
  const indexRootArgs = indexRoot ? ['--index-root', indexRoot] : [];

  // Binary resolution:
  // 1. Explicit config path.
  if (config.serverBinaryPath.trim() !== '') {
    return { cmd: config.serverBinaryPath.trim(), args: indexRootArgs, cwd };
  }

  // 2. ticket-viewer binary found inside the .ticket workspace's sibling target/debug/.
  //    This is the development-time path when working inside the context-engine repo.
  const binaryName = process.platform === 'win32' ? 'ticket-viewer.exe' : 'ticket-viewer';
  if (detected[0]?.folder.uri.fsPath) {
    const devBinary = path.join(detected[0].folder.uri.fsPath, 'target', 'debug', binaryName);
    if (fs.existsSync(devBinary)) {
      return { cmd: devBinary, args: indexRootArgs, cwd };
    }
  }

  // 3. ticket-viewer on the system PATH.
  const onPath = process.platform === 'win32' ? 'ticket-viewer.exe' : 'ticket-viewer';
  // We cannot check PATH existence cheaply, but `spawn` will throw if not found.
  return { cmd: onPath, args: indexRootArgs, cwd };
}

interface ServerHandle {
  process: ChildProcess;
  /** The URL the server is actually listening on (e.g. http://localhost:54321). */
  serverUrl: string;
}

/**
 * Spawn the ticket-viewer server with `--port 0` so the OS assigns a free
 * port.  The server prints `TICKET_VIEWER_PORT=<port>` on stdout once it has
 * bound; we parse that to discover the actual URL.
 *
 * Returns a promise that resolves once the port has been detected or rejects
 * on early exit / timeout.
 */
function startServerTask(
  outputChannel: vscode.OutputChannel,
  config: ReturnType<typeof readConfig>,
): Promise<ServerHandle> {
  const { cmd, args, cwd } = resolveServerLaunch(config);

  // Force --port 0 so we always get a fresh, conflict-free port.
  const finalArgs = [...args, '--port', '0'];

  outputChannel.appendLine(`[ticket-viewer] Starting: ${cmd} ${finalArgs.join(' ')}`);
  outputChannel.appendLine(`[ticket-viewer] Working directory: ${cwd ?? '(inherited)'}`);

  const proc = spawn(cmd, finalArgs, {
    cwd,
    detached: false,
    windowsHide: true,
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  return new Promise<ServerHandle>((resolve, reject) => {
    let resolved = false;
    const PORT_RE = /TICKET_VIEWER_PORT=(\d+)/;

    proc.stdout?.on('data', (d: Buffer) => {
      const text = d.toString();
      outputChannel.append(text);

      if (!resolved) {
        const m = PORT_RE.exec(text);
        if (m) {
          resolved = true;
          const port = Number(m[1]);
          const serverUrl = `http://localhost:${port}`;
          outputChannel.appendLine(`[ticket-viewer] Detected server on ${serverUrl}`);
          resolve({ process: proc, serverUrl });
        }
      }
    });

    proc.stderr?.on('data', (d: Buffer) => outputChannel.append(d.toString()));

    proc.on('error', err => {
      outputChannel.appendLine(`[ticket-viewer] Error: ${err.message}`);
      if (!resolved) {
        resolved = true;
        reject(err);
      }
    });

    proc.on('exit', code => {
      outputChannel.appendLine(`[ticket-viewer] Exited with code ${code}`);
      if (!resolved) {
        resolved = true;
        reject(new Error(`Server exited with code ${code} before reporting a port`));
      }
    });

    // Safety timeout: if the server hasn't printed its port within 30 s,
    // give up so the extension doesn't hang indefinitely.
    setTimeout(() => {
      if (!resolved) {
        resolved = true;
        reject(new Error('Timed out waiting for server to report its port'));
      }
    }, 30_000);
  });
}

/**
 * Returns true if the server at baseUrl responds to a workspaces request
 * within a short timeout. Used to detect an already-running server before
 * attempting to spawn a new one.
 */
async function pingServer(baseUrl: string): Promise<boolean> {
  try {
    const controller = new AbortController();
    const id = setTimeout(() => controller.abort(), 2000);
    const res = await fetch(`${baseUrl}/api/workspaces`, { signal: controller.signal });
    clearTimeout(id);
    return res.ok;
  } catch {
    return false;
  }
}

/**
 * Poll the server's health endpoint every 2 seconds until it responds with
 * a successful status or the timeout elapses. Resolves either way.
 */
async function pollUntilReachable(baseUrl: string, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const controller = new AbortController();
      const id = setTimeout(() => controller.abort(), 2000);
      const res = await fetch(`${baseUrl}/api/workspaces`, { signal: controller.signal });
      clearTimeout(id);
      if (res.ok) { return; }
    } catch { /* not ready yet */ }
    await new Promise<void>(resolve => setTimeout(resolve, 2000));
  }
}

let _serverProcess: import('node:child_process').ChildProcess | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  let config = readConfig();

  // ── Output channel for server logs ───────────────────────────────
  const outputChannel = vscode.window.createOutputChannel('Ticket Viewer Server');
  context.subscriptions.push(outputChannel);

  // ── Auto-start server & determine server URL ─────────────────────
  // When auto-starting, we always bind on a fresh port (--port 0) so we
  // never collide with other instances.  The configured serverUrl is only
  // used when auto-start is disabled (i.e. the user manages the server).
  let serverUrl = config.serverUrl;

  if (config.autoStartServer) {
    // If a server is already reachable on the configured URL (e.g. started by
    // viewer-ctl), skip auto-start entirely. The ticket store only allows one
    // opener at a time (exclusive SQLite write lock), so spawning a second instance
    // would immediately crash with exit code 101.
    if (await pingServer(config.serverUrl)) {
      outputChannel.appendLine(`[ticket-viewer] Existing server detected at ${config.serverUrl} — skipping auto-start.`);
      serverUrl = config.serverUrl;
    } else {
      try {
        const handle = await startServerTask(outputChannel, config);
        _serverProcess = handle.process;
        serverUrl = handle.serverUrl;
        vscode.window.setStatusBarMessage(`$(server) Ticket server running on ${serverUrl}`, 5000);
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        outputChannel.appendLine(`[ticket-viewer] Failed to start server: ${msg}`);
        void vscode.window.showWarningMessage(`Ticket Viewer server failed to start: ${msg}`);
      }
    }
  }

  let { workspace, displayName } = await resolveActiveWorkspace(
    serverUrl,
    config.workspace,
    context,
  );

  // ── Tree data provider ──────────────────────────────────────────
  const provider = new TicketTreeProvider(
    serverUrl,
    workspace,
    config.autoRefreshSeconds,
    resolveTicketsDir(workspace),
  );
  context.subscriptions.push(provider);

  // If we auto-started, the server is already listening (we waited for the
  // port), but give it a moment to finish workspace initialisation.
  if (config.autoStartServer && _serverProcess) {
    void pollUntilReachable(serverUrl, 30_000).then(() => provider.refresh());
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
  statusBarItem.tooltip = `Open Ticket Viewer (${serverUrl})`;
  statusBarItem.show();
  context.subscriptions.push(statusBarItem);

  function updateStatusBar(): void {
    const tickets = provider.allTickets;
    const newCount = tickets.filter(t => t.state === 'new').length;
    const inImplCount = tickets.filter(t => t.state === 'in-implementation').length;
    const prefix = `$(issues) ${displayName}`;

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

  // ── Commands ──────────────────────────────────────────────────────────────
  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.openBrowser', () => {
      openTicketViewer(serverUrl);
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.refresh', () => {
      provider.refresh();
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.startServer', async () => {
      // If a server is already running and responsive on the current URL,
      // just refresh the tree view rather than trying to spawn a duplicate.
      if (await pingServer(serverUrl)) {
        provider.refresh();
        vscode.window.setStatusBarMessage(`$(server) Server already running at ${serverUrl}`, 3000);
        return;
      }
      if (_serverProcess && !_serverProcess.killed) {
        _serverProcess.kill();
      }
      try {
        const handle = await startServerTask(outputChannel, config);
        _serverProcess = handle.process;
        serverUrl = handle.serverUrl;
        vscode.window.setStatusBarMessage(`$(server) Ticket server running on ${serverUrl}`, 5000);
        provider.update(serverUrl, workspace, config.autoRefreshSeconds, resolveTicketsDir(workspace));
        statusBarItem.tooltip = `Open Ticket Viewer (${serverUrl})`;
        void pollUntilReachable(serverUrl, 30_000).then(() => provider.refresh());
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        void vscode.window.showErrorMessage(`Failed to start server: ${msg}`);
      }
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      'ticket-viewer.openTicket',
      (item: TicketItem) => {
        // Prefer showing description.md as a scrollable markdown preview.
        const ticketsDir = resolveTicketsDir(workspace);
        if (ticketsDir) {
          const descPath = path.join(ticketsDir, item.ticket.id, 'description.md');
          if (fs.existsSync(descPath)) {
            void vscode.commands.executeCommand('markdown.showPreviewToSide', vscode.Uri.file(descPath));
            return;
          }
        }
        // Fallback: open browser viewer.
        const ticketUrl = `${serverUrl}/workspace/${encodeURIComponent(workspace)}/ticket/${encodeURIComponent(item.ticket.id)}`;
        openTicketViewer(ticketUrl);
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      'ticket-viewer.copyId',
      (item: TicketItem) => {
        void vscode.env.clipboard.writeText(item.ticket.id).then(() => {
          vscode.window.setStatusBarMessage(`$(check) Copied: ${item.ticket.id}`, 5000);
        });
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      'ticket-viewer.openInTicketViewer',
      (item: TicketItem) => {
        const ticketUrl = `${serverUrl}/workspace/${encodeURIComponent(workspace)}/ticket/${encodeURIComponent(item.ticket.id)}`;
        openTicketViewer(ticketUrl);
      },
    ),
  );

  // ── Ticket mutation commands ──────────────────────────────────────────────

  /** Helper: run a mutation and refresh, or show error. */
  async function runMutation(action: () => Promise<unknown>): Promise<void> {
    try {
      await action();
      provider.refresh();
    } catch (err) {
      void vscode.window.showErrorMessage(
        err instanceof Error ? err.message : String(err),
      );
    }
  }

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.createTicket', async () => {
      const typeItems = TICKET_TYPES.map(t => ({ label: t }));
      const typePick = await vscode.window.showQuickPick(typeItems, {
        title: 'New Ticket — Select Type',
        placeHolder: 'Ticket type',
      });
      if (!typePick) { return; }

      const title = await vscode.window.showInputBox({
        title: 'New Ticket — Title',
        prompt: 'Enter a title for the new ticket',
        ignoreFocusOut: true,
      });
      if (title === undefined) { return; }

      await runMutation(() => createTicket(serverUrl, workspace, typePick.label, title));
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      'ticket-viewer.editTitle',
      async (item: TicketItem) => {
        const newTitle = await vscode.window.showInputBox({
          title: 'Edit Title',
          value: item.ticket.title ?? '',
          prompt: 'New title for the ticket',
          ignoreFocusOut: true,
        });
        if (newTitle === undefined) { return; }
        await runMutation(() =>
          updateTicket(serverUrl, workspace, item.ticket.id, {
            fields: { title: newTitle },
          }),
        );
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      'ticket-viewer.setState',
      async (item: TicketItem) => {
        const current = item.ticket.state;
        const stateItems = TICKET_STATES.map(s => ({
          label: s,
          description: s === current ? '← current' : undefined,
        }));
        const pick = await vscode.window.showQuickPick(stateItems, {
          title: `Set State — ${item.ticket.title ?? item.ticket.id.slice(0, 8)}`,
          placeHolder: `Current: ${current ?? 'unknown'}`,
        });
        if (!pick || pick.label === current) { return; }
        await runMutation(() =>
          updateTicket(serverUrl, workspace, item.ticket.id, { state: pick.label }),
        );
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      'ticket-viewer.editDescription',
      (item: TicketItem) => {
        const ticketsDir = resolveTicketsDir(workspace);
        if (!ticketsDir) {
          void vscode.window.showWarningMessage('Ticket folder not found on disk.');
          return;
        }
        const descPath = path.join(ticketsDir, item.ticket.id, 'description.md');
        void vscode.commands.executeCommand('vscode.open', vscode.Uri.file(descPath));
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      'ticket-viewer.previewDescription',
      (ticketId: string) => {
        const ticketsDir = resolveTicketsDir(workspace);
        if (!ticketsDir) {
          void vscode.window.showWarningMessage('Ticket folder not found on disk.');
          return;
        }
        const descPath = path.join(ticketsDir, ticketId, 'description.md');
        if (!fs.existsSync(descPath)) {
          void vscode.window.showInformationMessage('No description.md found for this ticket.');
          return;
        }
        void vscode.commands.executeCommand('markdown.showPreviewToSide', vscode.Uri.file(descPath));
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      'ticket-viewer.closeTicket',
      async (item: TicketItem) => {
        const confirm = await vscode.window.showWarningMessage(
          `Fast-forward "${item.ticket.title ?? item.ticket.id.slice(0, 8)}" to done?`,
          { modal: true }, 'Close Ticket',
        );
        if (confirm !== 'Close Ticket') { return; }
        await runMutation(() => closeTicket(serverUrl, workspace, item.ticket.id));
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      'ticket-viewer.cancelTicket',
      async (item: TicketItem) => {
        const reason = await vscode.window.showInputBox({
          title: `Cancel — ${item.ticket.title ?? item.ticket.id.slice(0, 8)}`,
          prompt: 'Reason for cancellation (optional)',
          ignoreFocusOut: true,
        });
        if (reason === undefined) { return; }
        await runMutation(() =>
          cancelTicket(serverUrl, workspace, item.ticket.id, reason || undefined),
        );
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      'ticket-viewer.undoTicket',
      async (item: TicketItem) => {
        const confirm = await vscode.window.showWarningMessage(
          `Undo last transition on "${item.ticket.title ?? item.ticket.id.slice(0, 8)}"?`,
          { modal: true }, 'Undo',
        );
        if (confirm !== 'Undo') { return; }
        await runMutation(() => undoTicket(serverUrl, workspace, item.ticket.id));
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      'ticket-viewer.addDependency',
      async (item: TicketItem) => {
        const all = provider.allTickets.filter(t => t.id !== item.ticket.id);
        const picks = all.map(t => ({
          label: t.title ?? `(${t.id.slice(0, 8)})`,
          description: `${t.id.slice(0, 8)} · ${t.state ?? '?'}`,
          id: t.id,
        }));
        const pick = await vscode.window.showQuickPick(picks, {
          title: `Add Dependency to "${item.ticket.title ?? item.ticket.id.slice(0, 8)}"`,
          placeHolder: 'Select dependency ticket',
        });
        if (!pick) { return; }
        await runMutation(() =>
          addEdge(serverUrl, workspace, item.ticket.id, pick.id, 'depends_on'),
        );
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      'ticket-viewer.deleteTicket',
      async (item: TicketItem) => {
        const confirm = await vscode.window.showWarningMessage(
          `Delete "${item.ticket.title ?? item.ticket.id.slice(0, 8)}"? This cannot be undone.`,
          { modal: true }, 'Delete',
        );
        if (confirm !== 'Delete') { return; }
        await runMutation(() => deleteTicket(serverUrl, workspace, item.ticket.id));
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
      const resolved = await resolveActiveWorkspace(serverUrl, config.workspace, context);
      workspace = resolved.workspace;
      displayName = resolved.displayName;
      provider.update(serverUrl, workspace, config.autoRefreshSeconds, resolveTicketsDir(workspace));
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
        value: serverUrl,
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
      const resolved = await resolveActiveWorkspace(serverUrl, config.workspace, context);
      workspace = resolved.workspace;
      displayName = resolved.displayName;
      provider.update(serverUrl, workspace, config.autoRefreshSeconds, resolveTicketsDir(workspace));
      updateStatusBar();
    }),
  );

  // ── React to config changes ───────────────────────────────────────────
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(async e => {
      if (!e.affectsConfiguration('ticketViewer')) { return; }
      config = readConfig();
      // When auto-start is disabled the user manages the server, so honour
      // the configured URL.  When auto-start is enabled we keep the URL we
      // obtained at startup (or from the last manual startServer command).
      if (!config.autoStartServer) {
        serverUrl = config.serverUrl;
      }
      const resolved = await resolveActiveWorkspace(serverUrl, config.workspace, context);
      workspace = resolved.workspace;
      displayName = resolved.displayName;
      provider.update(serverUrl, workspace, config.autoRefreshSeconds, resolveTicketsDir(workspace));
      statusBarItem.tooltip = `Open Ticket Viewer (${serverUrl})`;
      updateStatusBar();
    }),
  );
}

export function deactivate(): void {
  // Kill the background server process if we started it.
  if (_serverProcess && !_serverProcess.killed) {
    _serverProcess.kill();
    _serverProcess = undefined;
  }
}

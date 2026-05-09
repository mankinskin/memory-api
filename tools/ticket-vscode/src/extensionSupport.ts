import * as vscode from 'vscode';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { spawn, type ChildProcess } from 'node:child_process';

import { fetchWorkspaces } from './api';

export const TICKET_STATES = [
  'new', 'ready', 'in-implementation',
  'in-review', 'done', 'cancelled',
];

export const TICKET_TYPES = ['tracker-improvement'];

export interface TicketViewerConfig {
  serverUrl: string;
  workspace: string;
  autoRefreshSeconds: number;
  autoStartServer: boolean;
  bridgePort: number;
  cdpPort: number;
  autoConnectCdp: boolean;
  serverBinaryPath: string;
  serverWorkingDirectory: string;
}

export function readConfig(): TicketViewerConfig {
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

export interface DetectedWorkspace {
  folderName: string;
  ticketPath: string;
  folder: vscode.WorkspaceFolder;
}

export interface ActiveWorkspace {
  workspace: string;
  displayName: string;
}

export interface ServerHandle {
  process: ChildProcess;
  serverUrl: string;
}

export function detectTicketWorkspaces(): DetectedWorkspace[] {
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

export async function resolveActiveWorkspace(
  serverUrl: string,
  configured: string,
  context: vscode.ExtensionContext,
): Promise<ActiveWorkspace> {
  if (configured.trim() !== '') {
    return { workspace: configured.trim(), displayName: configured.trim() };
  }

  const detected = detectTicketWorkspaces();

  let serverWorkspaces: string[] = [];
  try {
    const list = await fetchWorkspaces(serverUrl);
    serverWorkspaces = list.map(workspace => workspace.name);
  } catch { /* server may not be running yet */ }

  if (detected.length === 1) {
    const { folderName } = detected[0];
    const workspace = serverWorkspaces.includes(folderName)
      ? folderName
      : (serverWorkspaces[0] ?? 'default');
    return { workspace, displayName: folderName };
  }

  if (detected.length > 1) {
    const stored = context.workspaceState.get<string>('activeTicketFolder');
    if (stored) {
      const match = detected.find(candidate => candidate.folderName === stored);
      if (match) {
        const workspace = serverWorkspaces.includes(match.folderName)
          ? match.folderName
          : (serverWorkspaces[0] ?? 'default');
        return { workspace, displayName: match.folderName };
      }
    }

    const items = detected.map(candidate => ({
      label: candidate.folderName,
      description: candidate.ticketPath,
      folderName: candidate.folderName,
    }));
    const pick = await vscode.window.showQuickPick(items, {
      placeHolder: 'Multiple .ticket workspaces found — select one',
      title: 'Active Ticket Workspace',
    });
    if (pick) {
      await context.workspaceState.update('activeTicketFolder', pick.folderName);
      const workspace = serverWorkspaces.includes(pick.folderName)
        ? pick.folderName
        : (serverWorkspaces[0] ?? 'default');
      return { workspace, displayName: pick.folderName };
    }
  }

  const workspace = serverWorkspaces[0] ?? 'default';
  return { workspace, displayName: workspace };
}

export function openTicketViewer(url: string): void {
  void vscode.commands.executeCommand('simpleBrowser.show', url);
}

export function resolveTicketsDir(workspaceName: string): string | undefined {
  const detected = detectTicketWorkspaces();
  const match = detected.find(candidate => candidate.folderName === workspaceName) ?? detected[0];
  if (!match) { return undefined; }
  const dir = path.join(match.ticketPath, 'tickets');
  try {
    if (fs.statSync(dir).isDirectory()) { return dir; }
  } catch { /* not found */ }
  return undefined;
}

function resolveServerLaunch(config: TicketViewerConfig): {
  cmd: string;
  args: string[];
  cwd: string | undefined;
} {
  const detected = detectTicketWorkspaces();

  const cwd = config.serverWorkingDirectory.trim() !== ''
    ? config.serverWorkingDirectory.trim()
    : (detected[0]?.folder.uri.fsPath
        ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath);

  const indexRoot = detected[0]?.ticketPath ?? undefined;
  const indexRootArgs = indexRoot ? ['--index-root', indexRoot] : [];

  if (config.serverBinaryPath.trim() !== '') {
    return { cmd: config.serverBinaryPath.trim(), args: indexRootArgs, cwd };
  }

  const binaryName = process.platform === 'win32' ? 'ticket-viewer.exe' : 'ticket-viewer';
  if (detected[0]?.folder.uri.fsPath) {
    const devBinary = path.join(detected[0].folder.uri.fsPath, 'target', 'debug', binaryName);
    if (fs.existsSync(devBinary)) {
      return { cmd: devBinary, args: indexRootArgs, cwd };
    }
  }

  const onPath = process.platform === 'win32' ? 'ticket-viewer.exe' : 'ticket-viewer';
  return { cmd: onPath, args: indexRootArgs, cwd };
}

export function startServerTask(
  outputChannel: vscode.OutputChannel,
  config: TicketViewerConfig,
): Promise<ServerHandle> {
  const { cmd, args, cwd } = resolveServerLaunch(config);
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
    const portRe = /TICKET_VIEWER_PORT=(\d+)/;

    proc.stdout?.on('data', (data: Buffer) => {
      const text = data.toString();
      outputChannel.append(text);

      if (!resolved) {
        const match = portRe.exec(text);
        if (match) {
          resolved = true;
          const port = Number(match[1]);
          const serverUrl = `http://localhost:${port}`;
          outputChannel.appendLine(`[ticket-viewer] Detected server on ${serverUrl}`);
          resolve({ process: proc, serverUrl });
        }
      }
    });

    proc.stderr?.on('data', (data: Buffer) => outputChannel.append(data.toString()));

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

    setTimeout(() => {
      if (!resolved) {
        resolved = true;
        reject(new Error('Timed out waiting for server to report its port'));
      }
    }, 30_000);
  });
}

export async function pingServer(baseUrl: string): Promise<boolean> {
  try {
    const controller = new AbortController();
    const id = setTimeout(() => controller.abort(), 2000);
    const response = await fetch(`${baseUrl}/api/workspaces`, { signal: controller.signal });
    clearTimeout(id);
    return response.ok;
  } catch {
    return false;
  }
}

export async function pollUntilReachable(baseUrl: string, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const controller = new AbortController();
      const id = setTimeout(() => controller.abort(), 2000);
      const response = await fetch(`${baseUrl}/api/workspaces`, { signal: controller.signal });
      clearTimeout(id);
      if (response.ok) { return; }
    } catch { /* not ready yet */ }
    await new Promise<void>(resolve => setTimeout(resolve, 2000));
  }
}
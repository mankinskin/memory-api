import * as vscode from 'vscode';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { spawn, type ChildProcess } from 'node:child_process';

import { fetchWorkspaces, type WorkspaceInfo } from './api';

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

function normalizePath(value: string): string {
  return value.replace(/\\/g, '/').replace(/\/+$|\/$/g, '').toLowerCase();
}

function stripTicketSuffix(value: string): string {
  const normalized = normalizePath(value);
  return normalized.endsWith('/.ticket')
    ? normalized.slice(0, -'/.ticket'.length)
    : normalized;
}

function displayWorkspaceLabel(workspace: WorkspaceInfo): string {
  const label = workspace.label?.trim();
  return label && label !== '' ? label : workspace.name;
}

function matchesDetectedWorkspace(
  workspace: WorkspaceInfo,
  detected: DetectedWorkspace,
): boolean {
  const normalizedName = normalizePath(workspace.name);
  const normalizedLabel = normalizePath(workspace.label ?? '');
  const folderName = detected.folderName.trim().toLowerCase();
  const folderPath = normalizePath(detected.folder.uri.fsPath);
  const ticketPath = normalizePath(detected.ticketPath);

  if (workspace.name === detected.folderName) {
    return true;
  }

  if ((workspace.label ?? '').trim() === detected.folderName) {
    return true;
  }

  return [normalizedName, normalizedLabel].some(candidate => {
    if (candidate === '') {
      return false;
    }

    return candidate === folderPath
      || candidate === ticketPath
      || stripTicketSuffix(candidate) === folderPath
      || stripTicketSuffix(candidate) === ticketPath
      || candidate === folderName;
  });
}

function resolveWorkspaceSelection(
  serverWorkspaces: WorkspaceInfo[],
  activeWorkspace: string | undefined,
  detected: DetectedWorkspace | undefined,
): { workspace: string; displayName: string } {
  const activeName = activeWorkspace?.trim() || undefined;

  if (detected) {
    const exactMatch = serverWorkspaces.find(workspace =>
      matchesDetectedWorkspace(workspace, detected),
    );
    if (exactMatch) {
      return {
        workspace: exactMatch.name,
        displayName: detected.folderName,
      };
    }
  }

  if (activeName) {
    const activeMatch = serverWorkspaces.find(workspace => workspace.name === activeName);
    if (activeMatch) {
      return {
        workspace: activeMatch.name,
        displayName: detected?.folderName ?? displayWorkspaceLabel(activeMatch),
      };
    }
  }

  const fallback = serverWorkspaces[0];
  if (fallback) {
    return {
      workspace: fallback.name,
      displayName: detected?.folderName ?? displayWorkspaceLabel(fallback),
    };
  }

  const displayName = detected?.folderName ?? 'default';
  return { workspace: 'default', displayName };
}

function preferredBrowserCandidates(): string[] {
  if (process.platform === 'win32') {
    const roots = [
      process.env.PROGRAMFILES,
      process.env['PROGRAMFILES(X86)'],
      process.env.LOCALAPPDATA,
    ].filter((value): value is string => typeof value === 'string' && value !== '');

    return [
      'chrome.exe',
      'chromium.exe',
      'msedge.exe',
      ...roots.flatMap(root => [
        path.join(root, 'Google', 'Chrome', 'Application', 'chrome.exe'),
        path.join(root, 'Chromium', 'Application', 'chrome.exe'),
        path.join(root, 'Microsoft', 'Edge', 'Application', 'msedge.exe'),
      ]),
    ];
  }

  if (process.platform === 'darwin') {
    return [
      '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
      '/Applications/Chromium.app/Contents/MacOS/Chromium',
      '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge',
    ];
  }

  return [
    'google-chrome',
    'google-chrome-stable',
    'chromium-browser',
    'chromium',
    'microsoft-edge',
    'microsoft-edge-stable',
  ];
}

function resolvePreferredBrowserBinary(): string | undefined {
  for (const candidate of preferredBrowserCandidates()) {
    if (path.isAbsolute(candidate)) {
      if (fs.existsSync(candidate)) {
        return candidate;
      }
      continue;
    }

    const resolved = resolveBinaryOnPath(candidate);
    if (resolved) {
      return resolved;
    }
  }

  return undefined;
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

  let serverWorkspaces: WorkspaceInfo[] = [];
  let activeWorkspace: string | undefined;
  try {
    const response = await fetchWorkspaces(serverUrl);
    serverWorkspaces = response.workspaces;
    activeWorkspace = response.active_workspace;
  } catch { /* server may not be running yet */ }

  if (detected.length === 1) {
    return resolveWorkspaceSelection(serverWorkspaces, activeWorkspace, detected[0]);
  }

  if (detected.length > 1) {
    const stored = context.workspaceState.get<string>('activeTicketFolder');
    if (stored) {
      const match = detected.find(candidate => candidate.folderName === stored);
      if (match) {
        return resolveWorkspaceSelection(serverWorkspaces, activeWorkspace, match);
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
      const detectedMatch = detected.find(candidate => candidate.folderName === pick.folderName);
      return resolveWorkspaceSelection(serverWorkspaces, activeWorkspace, detectedMatch);
    }
  }

  return resolveWorkspaceSelection(serverWorkspaces, activeWorkspace, undefined);
}

export function openTicketViewer(url: string): void {
  const browserBinary = resolvePreferredBrowserBinary();
  if (!browserBinary) {
    void vscode.env.openExternal(vscode.Uri.parse(url));
    return;
  }

  const child = spawn(browserBinary, ['--new-window', '--start-fullscreen', url], {
    detached: true,
    stdio: 'ignore',
    windowsHide: true,
  });
  child.on('error', () => {
    void vscode.env.openExternal(vscode.Uri.parse(url));
  });
  child.unref();
}

export function resolveTicketsDir(workspaceName: string, displayName?: string): string | undefined {
  const detected = detectTicketWorkspaces();
  const normalizedWorkspace = normalizePath(workspaceName);
  const normalizedDisplay = displayName?.trim().toLowerCase();
  const match = detected.find(candidate => {
    const normalizedFolder = candidate.folderName.trim().toLowerCase();
    const normalizedFolderPath = normalizePath(candidate.folder.uri.fsPath);
    const normalizedTicketPath = normalizePath(candidate.ticketPath);

    return normalizedFolder === normalizedDisplay
      || normalizedFolder === workspaceName.trim().toLowerCase()
      || normalizedFolderPath === normalizedWorkspace
      || normalizedTicketPath === normalizedWorkspace
      || stripTicketSuffix(normalizedWorkspace) === normalizedFolderPath
      || stripTicketSuffix(normalizedWorkspace) === normalizedTicketPath;
  }) ?? detected[0];
  if (!match) { return undefined; }
  const dir = path.join(match.ticketPath, 'tickets');
  try {
    if (fs.statSync(dir).isDirectory()) { return dir; }
  } catch { /* not found */ }
  return undefined;
}

function resolveBinaryOnPath(binaryName: string): string | undefined {
  const pathValue = process.env.PATH ?? '';
  for (const entry of pathValue.split(path.delimiter)) {
    if (entry.trim() === '') {
      continue;
    }

    const candidate = path.join(entry, binaryName);
    try {
      const stat = fs.statSync(candidate);
      if (!stat.isFile()) {
        continue;
      }
      if (process.platform === 'win32' || (stat.mode & 0o111) !== 0) {
        return candidate;
      }
    } catch { /* not found */ }
  }

  return undefined;
}

export function resolveServerLaunch(config: TicketViewerConfig): {
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
  const pathBinary = resolveBinaryOnPath(binaryName);
  if (pathBinary) {
    return { cmd: pathBinary, args: indexRootArgs, cwd };
  }

  if (detected[0]?.folder.uri.fsPath) {
    const devBinary = path.join(detected[0].folder.uri.fsPath, 'target', 'debug', binaryName);
    if (fs.existsSync(devBinary)) {
      return { cmd: devBinary, args: indexRootArgs, cwd };
    }
  }

  return { cmd: binaryName, args: indexRootArgs, cwd };
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
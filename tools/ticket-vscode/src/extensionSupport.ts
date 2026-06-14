import * as vscode from 'vscode';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { execFile, spawn, type ChildProcess } from 'node:child_process';
import { promisify } from 'node:util';

import { fetchWorkspaces, type WorkspaceInfo } from './api';

const execFileAsync = promisify(execFile);
const LAST_SERVER_URL_KEY = 'ticketViewer.lastServerUrl';
const DISCOVERY_PROBE_TIMEOUT_MS = 400;
const DISCOVERY_BATCH_SIZE = 8;

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

function normalizeServerUrl(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  if (!trimmed) {
    return undefined;
  }

  try {
    const url = new URL(trimmed);
    if (url.protocol !== 'http:' && url.protocol !== 'https:') {
      return undefined;
    }

    const host = url.hostname === '127.0.0.1' || url.hostname === '::1'
      ? 'localhost'
      : url.hostname;
    const port = url.port ? `:${url.port}` : '';
    return `${url.protocol}//${host}${port}`;
  } catch {
    return undefined;
  }
}

function extractServerPort(value: string | undefined): number | undefined {
  const normalized = normalizeServerUrl(value);
  if (!normalized) {
    return undefined;
  }

  try {
    const url = new URL(normalized);
    const port = Number(url.port);
    return Number.isInteger(port) && port > 0 ? port : undefined;
  } catch {
    return undefined;
  }
}

export function parseListeningPorts(output: string): number[] {
  const ports: number[] = [];
  const seen = new Set<number>();

  for (const rawLine of output.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (line === '' || !/listen/i.test(line)) {
      continue;
    }

    const match = line.match(/(?:127\.0\.0\.1|0\.0\.0\.0|\[::1\]|::1|\*)[:.](\d+)\b/);
    if (!match) {
      continue;
    }

    const port = Number(match[1]);
    if (!Number.isInteger(port) || port <= 0 || seen.has(port)) {
      continue;
    }

    seen.add(port);
    ports.push(port);
  }

  return ports;
}

export function buildServerDiscoveryCandidates(
  configuredUrl: string,
  storedUrl: string | undefined,
  listeningPorts: number[],
  preferredUrls: string[] = [],
): string[] {
  const candidates: string[] = [];
  const seen = new Set<string>();
  const preferredPorts = [
    extractServerPort(preferredUrls[0]),
    extractServerPort(storedUrl),
    extractServerPort(configuredUrl),
    3002,
  ].filter((port): port is number => port !== undefined);

  function addCandidate(url: string | undefined): void {
    const normalized = normalizeServerUrl(url);
    if (!normalized || seen.has(normalized)) {
      return;
    }
    seen.add(normalized);
    candidates.push(normalized);
  }

  for (const url of preferredUrls) {
    addCandidate(url);
  }
  addCandidate(storedUrl);
  addCandidate(configuredUrl);

  for (const port of preferredPorts) {
    addCandidate(`http://localhost:${port}`);
  }

  for (const port of listeningPorts) {
    addCandidate(`http://localhost:${port}`);
  }

  return candidates;
}

async function listListeningPorts(): Promise<number[]> {
  const commands = process.platform === 'win32'
    ? [['netstat', ['-ano', '-p', 'tcp']] as const]
    : [['ss', ['-ltnH']] as const, ['lsof', ['-nP', '-iTCP', '-sTCP:LISTEN']] as const];

  for (const [command, args] of commands) {
    try {
      const { stdout } = await execFileAsync(command, args, {
        windowsHide: true,
        maxBuffer: 1024 * 1024,
      });
      const ports = parseListeningPorts(stdout);
      if (ports.length > 0) {
        return ports;
      }
    } catch {
      // Try the next available platform command.
    }
  }

  return [];
}

export async function rememberServerUrl(
  context: vscode.ExtensionContext,
  serverUrl: string,
): Promise<void> {
  const normalized = normalizeServerUrl(serverUrl);
  if (!normalized) {
    return;
  }
  await context.workspaceState.update(LAST_SERVER_URL_KEY, normalized);
}

export async function discoverRunningServerUrl(
  context: vscode.ExtensionContext,
  config: TicketViewerConfig,
  outputChannel?: vscode.OutputChannel,
  preferredUrls: string[] = [],
): Promise<string | undefined> {
  const storedUrl = context.workspaceState.get<string>(LAST_SERVER_URL_KEY);
  const listeningPorts = await listListeningPorts();
  const candidates = buildServerDiscoveryCandidates(
    config.serverUrl,
    storedUrl,
    listeningPorts,
    preferredUrls,
  );

  if (outputChannel && candidates.length > 0) {
    outputChannel.appendLine(`[ticket-viewer] Probing ${candidates.length} ticket-server candidate URL(s).`);
  }

  for (let index = 0; index < candidates.length; index += DISCOVERY_BATCH_SIZE) {
    const batch = candidates.slice(index, index + DISCOVERY_BATCH_SIZE);
    const results = await Promise.all(batch.map(async candidate => (
      await pingServer(candidate, DISCOVERY_PROBE_TIMEOUT_MS) ? candidate : undefined
    )));
    const discovered = results.find((candidate): candidate is string => candidate !== undefined);
    if (discovered) {
      if (outputChannel) {
        outputChannel.appendLine(`[ticket-viewer] Discovered running server at ${discovered}`);
      }
      await rememberServerUrl(context, discovered);
      return discovered;
    }
  }

  return undefined;
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
  const detected = detectTicketWorkspaces();
  const configuredName = configured.trim() || undefined;

  let serverWorkspaces: WorkspaceInfo[] = [];
  let activeWorkspace: string | undefined;
  try {
    const response = await fetchWorkspaces(serverUrl);
    serverWorkspaces = response.workspaces;
    activeWorkspace = response.active_workspace;
  } catch { /* server may not be running yet */ }

  if (configuredName) {
    const configuredMatch = serverWorkspaces.find(workspace => {
      if (workspace.name === configuredName) {
        return true;
      }

      return detected.some(candidate =>
        candidate.folderName === configuredName
        && matchesDetectedWorkspace(workspace, candidate),
      );
    });

    if (configuredMatch) {
      const detectedMatch = detected.find(candidate =>
        matchesDetectedWorkspace(configuredMatch, candidate),
      );
      return {
        workspace: configuredMatch.name,
        displayName: detectedMatch?.folderName ?? displayWorkspaceLabel(configuredMatch),
      };
    }

    if (serverWorkspaces.length === 0) {
      return { workspace: configuredName, displayName: configuredName };
    }
  }

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

/**
 * Open the ticket-viewer URL in an external browser.
 *
 * Rule 4 (frozen, spec ticket-vscode/rust-wasm-port): viewer navigation routes
 * through ExternalUrlCapability using asExternalUri so it is remote/Codespaces-
 * safe by default.  Pass an ExternalUrlCapability from hostCapabilities.ts when
 * available.  Falls back to the legacy desktop-binary path for backwards
 * compatibility until bfafde19 is fully rolled out.
 *
 * @deprecated Pass an ExternalUrlCapability instead.  The capability-based
 *   path (`capability.openExternal(url)`) should be preferred for all new
 *   callers; this function retains the Node-specific browser-binary spawn path
 *   for the desktop host only, and is excluded from the browser bundle.
 */
export function openTicketViewer(url: string): void {
  const browserBinary = resolvePreferredBrowserBinary();
  if (!browserBinary) {
    // Route through asExternalUri for remote/Codespaces-safe navigation.
    void (async () => {
      const external = await vscode.env.asExternalUri(vscode.Uri.parse(url));
      void vscode.env.openExternal(external);
    })();
    return;
  }

  const child = spawn(browserBinary, ['--new-window', '--start-fullscreen', url], {
    detached: true,
    stdio: 'ignore',
    windowsHide: true,
  });
  child.on('error', () => {
    void (async () => {
      const external = await vscode.env.asExternalUri(vscode.Uri.parse(url));
      void vscode.env.openExternal(external);
    })();
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

/**
 * URI-returning wrapper around resolveTicketsDir for the TicketTreeProvider.
 *
 * Rule 3 (frozen): TicketTreeProvider now accepts vscode.Uri instead of a
 * raw string path so virtual/web workspaces resolve correctly.  This helper
 * converts the desktop string path to a file URI; call sites in extension.ts
 * and extensionCommands.ts import this instead of resolveTicketsDir.
 */
export function resolveTicketsDirUri(workspaceName: string, displayName?: string): vscode.Uri | undefined {
  const dir = resolveTicketsDir(workspaceName, displayName);
  return dir !== undefined ? vscode.Uri.file(dir) : undefined;
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

export async function pingServer(baseUrl: string, timeoutMs = 2000): Promise<boolean> {
  try {
    const controller = new AbortController();
    const id = setTimeout(() => controller.abort(), timeoutMs);
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
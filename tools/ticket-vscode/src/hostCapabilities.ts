// Host capability contract for ticket-vscode.
//
// This module defines the TypeScript interfaces that form the seam between
// the JS/TS host shell and the Rust/WASM core.  The core never imports vscode
// or Node modules; all host-specific behaviour is routed through these adapters.
//
// Frozen contract — spec ticket-vscode/rust-wasm-port (a592900c), section
// "Host Capability Contract".  Follow-on tickets must not re-decide these
// shapes; amend the spec before changing this file.
//
// Rules (frozen):
// 1. Core depends only on required capabilities. Optional capabilities are
//    nullable; core derives feature availability from their presence.
// 2. Host detection uses env.uiKind / extension.extensionKind / env.remoteName
//    / workspace.isVirtualWorkspace — never process or navigator sniffing.
// 3. All filesystem access goes through WorkspaceFsCapability (URI-based).
//    No shared-shell code may import node:fs / node:path.
// 4. Viewer navigation always routes through ExternalUrlCapability via
//    asExternalUri so it is remote/Codespaces-safe by default.
// 5. Absent capability resolves to undefined; shell hides/disables the
//    corresponding command/menu.

import * as vscode from 'vscode';

// ── Required capabilities ─────────────────────────────────────────────────────

/** Fetch ticket API data and enumerate workspaces over HTTP. */
export interface FetchCapability {
  fetch(url: string, init?: RequestInit): Promise<Response>;
}

/** List ticket workspaces (HTTP + optional FS scan). */
export interface WorkspaceDiscoveryCapability {
  /** Enumerate workspaces from the running HTTP server. */
  listFromServer(baseUrl: string): Promise<string[]>;
  /** Scan VS Code workspace folders for .ticket directories. */
  scanWorkspaceFolders(): Promise<Array<{ folderName: string; ticketPath: string; folder: vscode.WorkspaceFolder }>>;
}

/** Read/list ticket files through VS Code URIs. */
export interface WorkspaceFsCapability {
  readDirectory(uri: vscode.Uri): Promise<[string, vscode.FileType][]>;
  readFile(uri: vscode.Uri): Promise<Uint8Array>;
  stat(uri: vscode.Uri): Promise<vscode.FileStat>;
}

/** Copy text to clipboard via vscode.env.clipboard. */
export interface ClipboardCapability {
  writeText(text: string): Promise<void>;
}

/** Open ticket-viewer / external URLs safely across all hosts. */
export interface ExternalUrlCapability {
  /** Wraps vscode.env.asExternalUri then vscode.env.openExternal. */
  openExternal(url: string): Promise<void>;
  /** Converts a local URI to an externally-reachable URI (port forwarding on remote). */
  asExternalUri(uri: vscode.Uri): Promise<vscode.Uri>;
}

/** Surface info/warn/error messages and prompts. */
export interface NotificationCapability {
  showInformation(message: string): void;
  showWarning(message: string): void;
  showError(message: string): void;
  showInputBox(options: vscode.InputBoxOptions): Promise<string | undefined>;
  showQuickPick(items: string[], options?: vscode.QuickPickOptions): Promise<string | undefined>;
}

/** Host-mode detection — never use process or navigator sniffing. */
export interface HostDetectionCapability {
  readonly uiKind: vscode.UIKind;
  readonly extensionKind: vscode.ExtensionKind;
  readonly remoteName: string | undefined;
  readonly isVirtualWorkspace: boolean;
  readonly isTrusted: boolean;
}

// ── Optional capabilities ─────────────────────────────────────────────────────

/** Start/connect a local ticket-viewer server (desktop/remote only). */
export interface ServerControlCapability {
  start(config: ServerControlConfig): Promise<ServerHandle>;
  discover(context: vscode.ExtensionContext, config: ServerControlConfig, outputChannel: vscode.OutputChannel): Promise<string | undefined>;
  stop(handle: ServerHandle): void;
}

export interface ServerControlConfig {
  serverUrl: string;
  serverBinaryPath: string;
  serverWorkingDirectory: string;
  autoStartServer: boolean;
}

export interface ServerHandle {
  serverUrl: string;
  dispose(): void;
}

/** CDP/Simple-Browser automation (desktop opt-in only). */
export interface BrowserBridgeCapability {
  start(controlPort: number, cdpPort: number, autoConnect: boolean): Promise<void>;
  stop(): void;
  navigate(url: string): Promise<void>;
  connectCdp(port?: number): Promise<void>;
  getStatus(): BrowserBridgeStatus;
}

export interface BrowserBridgeStatus {
  controlPort: number;
  cdpConnected: boolean;
  currentUrl: string | null;
}

// ── Aggregate HostCapabilities ────────────────────────────────────────────────

/** All host capabilities bundled for extension activation. */
export interface HostCapabilities {
  // Required
  fetch: FetchCapability;
  workspaceDiscovery: WorkspaceDiscoveryCapability;
  workspaceFs: WorkspaceFsCapability;
  clipboard: ClipboardCapability;
  externalUrl: ExternalUrlCapability;
  notifications: NotificationCapability;
  hostDetection: HostDetectionCapability;
  // Optional — undefined when the host does not support the capability
  serverControl?: ServerControlCapability;
  browserBridge?: BrowserBridgeCapability;
}

// ── HostKind derivation ───────────────────────────────────────────────────────

/** The four host modes from the spec. */
export type HostKind = 'desktop-node' | 'remote-workspace' | 'browser-web' | 'virtual';

/**
 * Derive the host kind from VS Code's runtime signals.
 *
 * Rule 2 (frozen): use only env.uiKind, extensionKind, env.remoteName, and
 * workspace.isVirtualWorkspace.  Never use process.env or navigator sniffing.
 */
export function detectHostKind(detection: HostDetectionCapability): HostKind {
  if (detection.isVirtualWorkspace) {
    return 'virtual';
  }
  if (detection.uiKind === vscode.UIKind.Web) {
    return 'browser-web';
  }
  if (detection.remoteName !== undefined) {
    return 'remote-workspace';
  }
  return 'desktop-node';
}

// ── Desktop implementations ───────────────────────────────────────────────────
//
// These concrete implementations are only included in the `main` (Node) host
// entrypoint.  The `browser` entrypoint does not import this file for any
// class that uses node:fs, node:path, or node:child_process.

/** Standard FetchCapability implementation using the global fetch. */
export class GlobalFetchCapability implements FetchCapability {
  fetch(url: string, init?: RequestInit): Promise<Response> {
    return globalThis.fetch(url, init);
  }
}

/** ClipboardCapability backed by vscode.env.clipboard. */
export class VsCodeClipboardCapability implements ClipboardCapability {
  writeText(text: string): Promise<void> {
    return Promise.resolve(vscode.env.clipboard.writeText(text));
  }
}

/**
 * ExternalUrlCapability using vscode.env.asExternalUri + openExternal.
 *
 * Rule 4 (frozen): all viewer navigation routes through this path so it is
 * remote/Codespaces-safe by default.
 */
export class VsCodeExternalUrlCapability implements ExternalUrlCapability {
  async openExternal(url: string): Promise<void> {
    const uri = vscode.Uri.parse(url);
    const external = await vscode.env.asExternalUri(uri);
    await vscode.env.openExternal(external);
  }

  asExternalUri(uri: vscode.Uri): Promise<vscode.Uri> {
    return Promise.resolve(vscode.env.asExternalUri(uri));
  }
}

/** NotificationCapability backed by vscode.window. */
export class VsCodeNotificationCapability implements NotificationCapability {
  showInformation(message: string): void {
    void vscode.window.showInformationMessage(message);
  }
  showWarning(message: string): void {
    void vscode.window.showWarningMessage(message);
  }
  showError(message: string): void {
    void vscode.window.showErrorMessage(message);
  }
  showInputBox(options: vscode.InputBoxOptions): Promise<string | undefined> {
    return Promise.resolve(vscode.window.showInputBox(options));
  }
  showQuickPick(items: string[], options?: vscode.QuickPickOptions): Promise<string | undefined> {
    return Promise.resolve(vscode.window.showQuickPick(items, options));
  }
}

/** WorkspaceFsCapability backed by vscode.workspace.fs. */
export class VsCodeWorkspaceFsCapability implements WorkspaceFsCapability {
  readDirectory(uri: vscode.Uri): Promise<[string, vscode.FileType][]> {
    return Promise.resolve(vscode.workspace.fs.readDirectory(uri));
  }
  readFile(uri: vscode.Uri): Promise<Uint8Array> {
    return Promise.resolve(vscode.workspace.fs.readFile(uri));
  }
  stat(uri: vscode.Uri): Promise<vscode.FileStat> {
    return Promise.resolve(vscode.workspace.fs.stat(uri));
  }
}

/**
 * HostDetectionCapability snapshot taken at activation time.
 *
 * Must be constructed once in the extension entrypoint and passed through;
 * do not construct on every call since extensionKind is a property of the
 * Extension object.
 */
export class VsCodeHostDetectionCapability implements HostDetectionCapability {
  readonly uiKind: vscode.UIKind;
  readonly extensionKind: vscode.ExtensionKind;
  readonly remoteName: string | undefined;
  readonly isVirtualWorkspace: boolean;
  readonly isTrusted: boolean;

  constructor(context: vscode.ExtensionContext) {
    this.uiKind = vscode.env.uiKind;
    this.extensionKind = context.extension.extensionKind;
    this.remoteName = vscode.env.remoteName || undefined;
    this.isVirtualWorkspace = vscode.workspace.workspaceFolders?.some(
      f => f.uri.scheme !== 'file',
    ) ?? false;
    this.isTrusted = vscode.workspace.isTrusted;
  }
}

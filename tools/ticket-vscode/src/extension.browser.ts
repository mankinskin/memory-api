// Browser/web extension host entrypoint for ticket-vscode.
//
// This file is bundled by esbuild into a single WebWorker-compatible file:
//   out/extension.browser.js
//
// Constraints (VS Code web extension host):
//   - No `require()` calls (only the vscode module is available)
//   - No `node:fs`, `node:path`, `node:child_process`, `node:http`, etc.
//   - Single-file bundle; dynamic import() of external paths is not available
//   - `vscode.workspace.fs` is available for file access through URIs
//   - `fetch` is available as a web global
//
// The WASM core is loaded from the extension package via vscode.workspace.fs
// so the same loader works in both the Node `main` host and this web host.

import * as vscode from 'vscode';
import { TicketTreeProvider } from './ticketProvider';
import { initWasmCore, type CoreApi } from './coreLoader';
import { detectHostKind } from './hostCapabilities';
import { fetchWorkspaces } from './api';

// ── Browser-safe config helpers ───────────────────────────────────────────────
// These inline the parts of extensionSupport.ts that are browser-compatible.
// extensionSupport.ts imports node:fs/path/child_process which cannot be
// bundled for the web extension host.

interface BrowserConfig {
  serverUrl: string;
  workspace: string;
  autoRefreshSeconds: number;
}

function readConfig(): BrowserConfig {
  const cfg = vscode.workspace.getConfiguration('ticketViewer');
  return {
    serverUrl: cfg.get<string>('serverUrl', 'http://localhost:3002'),
    workspace: cfg.get<string>('workspace', ''),
    autoRefreshSeconds: cfg.get<number>('autoRefreshSeconds', 30),
  };
}

async function resolveActiveWorkspace(
  serverUrl: string,
  configured: string,
): Promise<{ workspace: string; displayName: string }> {
  const configuredName = configured.trim();
  if (configuredName) {
    return { workspace: configuredName, displayName: configuredName };
  }
  try {
    const response = await fetchWorkspaces(serverUrl);
    const ws = response.active_workspace ?? response.workspaces[0]?.name ?? 'default';
    return { workspace: ws, displayName: ws };
  } catch {
    return { workspace: 'default', displayName: 'default' };
  }
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  // ── Load WASM core ─────────────────────────────────────────────────────────
  let core: CoreApi | undefined;
  try {
    const wasmUri = vscode.Uri.joinPath(
      context.extensionUri, 'pkg', 'ticket_vscode_core_bg.wasm',
    );
    const wasmBytes = await vscode.workspace.fs.readFile(wasmUri);
    core = await initWasmCore(wasmBytes);
    void vscode.window.showInformationMessage(
      `[ticket-vscode] browser host active — core ${core.core_version()}`,
    );
  } catch (err) {
    void vscode.window.showWarningMessage(
      `[ticket-vscode] WASM core failed to load in browser host: ${String(err)}`,
    );
  }

  // ── Host-kind detection and capability gating ──────────────────────────────
  const hostKind = detectHostKind({
    uiKind: vscode.env.uiKind,
    extensionKind: context.extension.extensionKind,
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

  // ── Provider activation ────────────────────────────────────────────────────
  const config = readConfig();

  const resolved = await resolveActiveWorkspace(config.serverUrl, config.workspace);

  const provider = new TicketTreeProvider(
    config.serverUrl,
    resolved.workspace,
    config.autoRefreshSeconds,
    undefined, // no local .ticket directory in browser host
    undefined, // no server-recovery in browser host
    undefined, // no WorkspaceFsCapability for file browsing (browser host)
    core,
  );
  context.subscriptions.push(provider);

  const treeView = vscode.window.createTreeView('ticket-viewer.tickets', {
    treeDataProvider: provider,
    showCollapseAll: true,
  });
  context.subscriptions.push(treeView);

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.refresh', () => {
      provider.refresh();
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.browserHostInfo', () => {
      const uiKind = vscode.env.uiKind === vscode.UIKind.Web ? 'web' : 'desktop';
      const remoteName = vscode.env.remoteName ?? '(none)';
      void vscode.window.showInformationMessage(
        `[ticket-vscode] host: ${uiKind}, remote: ${remoteName}, kind: ${hostKind}`,
      );
    }),
  );
}

export function deactivate(): void {
  // Nothing to clean up — subscriptions are released by VS Code.
}

// WorkspaceDiscovery adapter for desktop/Node extension hosts.
//
// This module is imported ONLY from the `main` (Node/Electron) entrypoint.
// The `browser` entrypoint uses an HTTP-only workspace discovery path.
//
// Node-specific imports (node:fs, node:path) are intentionally confined here
// so they never appear in the shared shell or the browser bundle.
//
// Frozen contract: spec ticket-vscode/rust-wasm-port (a592900c),
// "Module Portability Matrix" row for extensionSupport.ts workspace discovery.

import * as vscode from 'vscode';
import * as fs from 'node:fs';
import * as path from 'node:path';

import type { WorkspaceDiscoveryCapability } from './hostCapabilities';
import { fetchWorkspaces } from './api';

export interface DetectedWorkspace {
  folderName: string;
  ticketPath: string;
  folder: vscode.WorkspaceFolder;
}

/**
 * Desktop/Node implementation of WorkspaceDiscoveryCapability.
 *
 * - listFromServer: calls the running ticket-viewer HTTP API.
 * - scanWorkspaceFolders: walks VS Code workspace folders looking for a
 *   `.ticket/` subdirectory using node:fs (desktop/remote only).
 */
export class DesktopWorkspaceDiscovery implements WorkspaceDiscoveryCapability {
  async listFromServer(baseUrl: string): Promise<string[]> {
    try {
      const response = await fetchWorkspaces(baseUrl);
      return response.workspaces.map(w => w.name);
    } catch {
      return [];
    }
  }

  async scanWorkspaceFolders(): Promise<DetectedWorkspace[]> {
    const folders = vscode.workspace.workspaceFolders ?? [];
    const results: DetectedWorkspace[] = [];
    for (const folder of folders) {
      // Only scan file-scheme URIs — virtual/web workspaces use HTTP only.
      if (folder.uri.scheme !== 'file') { continue; }
      const ticketPath = path.join(folder.uri.fsPath, '.ticket');
      if (fs.existsSync(ticketPath)) {
        results.push({ folderName: folder.name, ticketPath, folder });
      }
    }
    return results;
  }
}

/**
 * HTTP-only WorkspaceDiscovery for browser/virtual/remote extension hosts.
 *
 * Skips the node:fs scan entirely so it can be safely included in the web
 * bundle without importing Node modules.
 */
export class HttpOnlyWorkspaceDiscovery implements WorkspaceDiscoveryCapability {
  async listFromServer(baseUrl: string): Promise<string[]> {
    try {
      const response = await fetchWorkspaces(baseUrl);
      return response.workspaces.map(w => w.name);
    } catch {
      return [];
    }
  }

  async scanWorkspaceFolders(): Promise<DetectedWorkspace[]> {
    return [];
  }
}

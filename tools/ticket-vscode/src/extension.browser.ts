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

// ---------------------------------------------------------------------------
// WASM core loader
// ---------------------------------------------------------------------------

/**
 * Loads the ticket-vscode-core WASM binary from the extension package and
 * returns the instantiated WebAssembly exports.
 *
 * This loader uses `vscode.workspace.fs.readFile` against the extension URI,
 * which is available in both the desktop Node host and the web WebWorker host.
 * It avoids `fetch()` against a localhost URL (which would be blocked by CORS
 * in the browser host) and avoids `fs.readFileSync` (not available in the web
 * host).
 */
async function loadWasmCore(
  context: vscode.ExtensionContext,
): Promise<WebAssembly.Exports> {
  const wasmUri = vscode.Uri.joinPath(
    context.extensionUri,
    'pkg',
    'ticket_vscode_core_bg.wasm',
  );
  const wasmBytes = await vscode.workspace.fs.readFile(wasmUri);

  // Pass wasmBytes.buffer (ArrayBuffer) to disambiguate the
  // WebAssembly.instantiate overload (-> WebAssemblyInstantiatedSource)
  // from the Module overload (-> Instance).
  const result = await WebAssembly.instantiate(wasmBytes.buffer, {});
  return result.instance.exports;
}

// ---------------------------------------------------------------------------
// Activation
// ---------------------------------------------------------------------------

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  let coreExports: WebAssembly.Exports | undefined;

  try {
    coreExports = await loadWasmCore(context);
  } catch (err) {
    // Activation must not throw — log and degrade gracefully.
    void vscode.window.showWarningMessage(
      `[ticket-vscode] WASM core failed to load in browser host: ${String(err)}`,
    );
  }

  // Smoke-check: call core_version() if the export is available.
  if (coreExports && typeof coreExports['core_version'] === 'function') {
    const version = (coreExports['core_version'] as () => string)();
    void vscode.window.showInformationMessage(
      `[ticket-vscode] browser host active — core ${version}`,
    );
  }

  // Register a minimal command so VS Code can confirm the browser entry
  // activates and its contributions are visible.
  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.browserHostInfo', () => {
      const uiKind = vscode.env.uiKind === vscode.UIKind.Web ? 'web' : 'desktop';
      const remoteName = vscode.env.remoteName ?? '(none)';
      void vscode.window.showInformationMessage(
        `[ticket-vscode] host: ${uiKind}, remote: ${remoteName}`,
      );
    }),
  );

  // TODO (ticket bfafde19): register the full capability adapter set and
  // forward to the shared activation core once the host capability contract
  // is implemented.
}

export function deactivate(): void {
  // Nothing to clean up in the browser entry yet.
}

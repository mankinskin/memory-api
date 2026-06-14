#!/usr/bin/env node
// Bundle the browser extension entrypoint with esbuild.
//
// Output: out/extension.browser.js (single-file WebWorker-compatible bundle)
//
// The WASM asset (pkg/ticket_vscode_core_bg.wasm) is NOT inlined; it stays as
// a separate file in the extension package and is loaded at runtime via
// vscode.workspace.fs (see src/extension.browser.ts).
//
// Run via:  node scripts/bundle-browser.mjs  (or npm run bundle:browser)

import { build } from 'esbuild';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const require = createRequire(import.meta.url);
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '..');

await build({
  entryPoints: [path.join(root, 'src', 'extension.browser.ts')],
  bundle: true,
  outfile: path.join(root, 'out', 'extension.browser.js'),
  platform: 'browser',
  format: 'cjs',
  target: 'es2020',
  // Mark vscode as external — it is provided by the extension host at runtime.
  external: ['vscode'],
  // Do NOT bundle .wasm files; they are separate assets loaded at runtime.
  loader: { '.wasm': 'empty' },
  sourcemap: true,
  minify: false,
  logLevel: 'info',
});

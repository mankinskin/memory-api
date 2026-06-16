#!/usr/bin/env node
// Bundle the Node/desktop extension entrypoint with esbuild.
//
// Output: out/extension.js  (replaces the tsc-compiled output)
//
// The WASM asset (pkg/ticket_vscode_core_bg.wasm) stays as a separate file;
// it is loaded at runtime via vscode.workspace.fs (see src/coreLoader.ts).
// The wasm-bindgen JS glue (pkg/ticket_vscode_core_bg.js) IS bundled so the
// WASM can be initialised in both the Node and browser hosts identically.
//
// Run via:  node scripts/bundle-node.mjs  (or npm run bundle:node)

import { build } from 'esbuild';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '..');

await build({
  entryPoints: [path.join(root, 'src', 'extension.ts')],
  bundle: true,
  outfile: path.join(root, 'out', 'extension.js'),
  platform: 'node',
  format: 'cjs',
  target: 'node18',
  // Mark vscode as external — it is provided by the extension host at runtime.
  // Mark all node_modules as external — they are shipped alongside the extension
  // and resolved at runtime. This prevents bundling test-only deps like playwright.
  external: ['vscode'],
  packages: 'external',
  // Do NOT bundle .wasm files; they are separate assets loaded at runtime.
  loader: { '.wasm': 'empty' },
  sourcemap: true,
  minify: false,
  logLevel: 'info',
});

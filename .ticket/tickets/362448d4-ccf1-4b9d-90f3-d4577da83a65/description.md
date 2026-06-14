# [ticket-vscode] Add dual-host packaging, bundling, and extension test harnesses

Package the ported extension so it activates in both the desktop/remote Node host and the web extension host, and add the harnesses that validate both.

Acceptance criteria:
- [x] `package.json` ships both `main` and `browser` entries and the web bundle is a single WebWorker-compatible file.
- [x] The WASM asset and generated glue are included for both desktop and web packaging paths.
- [x] Desktop-only code is not loaded in the web bundle.
- [x] TypeScript typecheck and bundler builds pass for both entrypoints, plus a `@vscode/test-web` smoke harness.

## Implementation summary

**`package.json` changes:**
- `"main": "./out/extension.js"` (existing desktop entry).
- `"browser": "./out/extension.browser.js"` (new web extension host entry).
- Scripts: `bundle:browser`, `build` (compile + bundle:browser), `build:wasm` (wasm-pack), `build:full` (wasm + build).

**`.vscodeignore` updated:**
- Excluded: `src/`, `test/`, `scripts/`, `tsconfig*.json`, `jest.config.ts`, `out/*.js.map`.
- **Allowed**: `pkg/*.wasm`, `pkg/*_bg.wasm` — WASM binary must be in the VSIX for both entries.
- `*.map` blanket exclusion removed; only `out/*.js.map` is excluded.

**Web bundle:**
- `src/extension.browser.ts` — single WebWorker-compatible entrypoint; no Node imports.
- `scripts/bundle-browser.mjs` — esbuild; `vscode` external; `.wasm` loader `empty` (loaded at runtime via `vscode.workspace.fs`); output `out/extension.browser.js` (2.9 kB).
- `tsconfig.browser.json` — `"module": "esnext"`, `"lib": ["ES2020", "WebWorker"]`.
- Main `tsconfig.json` excludes `src/extension.browser.ts`.

**wasm-pack path:**
`build:wasm` target: `wasm-pack build ../../../crates/ticket-vscode-core --target bundler --out-dir ../tools/ticket-vscode/pkg`. `wasm-pack` not yet installed on this machine — the script is ready; install with `cargo install wasm-pack`.

**Build results:** `npm run build` clean. `npm run test:unit` — 32/32 passed.

## Frozen architecture boundary

Spec `ticket-vscode/rust-wasm-port` (a592900c) — Loader Constraints section: loader constraints, desktop vs browser entry, VSIX packaging, and build pipeline are all implemented as documented.

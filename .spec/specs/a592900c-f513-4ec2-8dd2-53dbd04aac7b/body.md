# Ticket-vscode Rust/WASM Port

## Goal

Port the current `memory-viewers/memory-api/tools/ticket-vscode` extension to a Rust/WASM-backed implementation without breaking the existing ticket browsing workflow.

The target is not a zero-JavaScript extension. VS Code still requires JavaScript entrypoints for extension activation and API calls. The practical target is a dual-host extension with a thin JS/TS shell and a Rust/WASM core.

## Research Findings

### VS Code runtime constraints

- A web-compatible VS Code extension must provide a `browser` entry in `package.json`.
- The `browser` entry runs inside a WebWorker and must be bundled into a single file.
- In the web extension host, `require('vscode')` is supported, but general module loading is not.
- Node globals and modules such as `process`, `path`, `fs`, and `child_process` are not available in the browser host.
- Workspace and extension files must be accessed through `vscode.workspace.fs` and URI-based APIs.
- Running child processes or local binaries is not possible in the browser host.
- Remote and Codespaces scenarios require `vscode.env.openExternal`, `vscode.env.asExternalUri`, and `vscode.env.clipboard` instead of local process-based helpers.

### Current ticket-vscode architecture findings

The current TypeScript extension mixes portable logic with Node-specific host behavior:

- `src/api.ts` is mostly portable HTTP/data-shape code.
- `src/ticketProvider.ts` contains portable filtering/grouping/root-detection logic, but also directly uses `node:fs` and `node:path` to expose on-disk ticket files under each ticket node.
- `src/extensionSupport.ts` is strongly Node-bound: config helpers, `.ticket` workspace discovery with `fs/path`, browser binary discovery with `process.env`, and `child_process.spawn()` for browser launching and server startup.
- `src/browserBridge.ts` is desktop-only: local HTTP control server, CDP probing, Simple Browser control, and Playwright-over-CDP automation.
- `src/extensionCommands.ts` currently combines VS Code command wiring with direct filesystem/process assumptions.

### Portability conclusion

A full rewrite of the extension surface in Rust is not realistic if the goal includes web and remote support. The viable split is:

- JS/TS host layer:
  - VS Code activation/deactivation
  - command registration
  - TreeItem creation and contribution wiring
  - capability adapters for clipboard, URIs, workspace FS, notifications, and host detection
  - any remaining desktop-only features
- Rust/WASM core:
  - ticket/edge/schema data models
  - filtering, grouping, root-ticket detection, and tree model derivation
  - URL and command intent derivation
  - capability-aware feature gating decisions
  - deterministic state transformations that can be unit-tested outside VS Code

## Target Architecture

### Runtime model

Ship both:

- `main` desktop entry for Node/Electron and remote workspace hosts
- `browser` entry for the web extension host

Both entries should call into the same Rust/WASM core through a narrow adapter boundary.

### Capability boundary

Define a host capability contract before porting behavior:

- fetch ticket API data
- enumerate workspaces
- read/list ticket files through VS Code URIs
- copy to clipboard
- open external URLs
- optionally start or connect to a ticket-viewer server when the host supports it
- detect host mode: desktop node, remote workspace, browser/web, virtual workspace

### Feature policy

The spec work must explicitly classify every existing feature into one of three buckets:

1. portable and required in all hosts
2. host-adapted with different implementations per host
3. desktop-only or deferred because web hosts cannot support it directly

Initial candidates:

- Portable: ticket list loading, filter/search state, tree derivation, selected-ticket URL creation, copy-ID intent
- Host-adapted: workspace discovery, local ticket file browsing, open-in-viewer behavior, server reachability checks
- Desktop-only or deferred: Browser Bridge control server, CDP automation, local browser binary selection, direct process spawning

## Planned Work Track

1. Write the target architecture spec and feature matrix.
2. Prove the extension can load a Rust/WASM module from both desktop and web extension hosts.
3. Extract portable domain logic into a new Rust crate compiled to wasm32.
4. Introduce host capability adapters and redesign Node-only behaviors for web/remote compatibility.
5. Add dual-host packaging, bundling, and test harnesses.
6. Validate parity across desktop, web, and remote-oriented scenarios.

## Validation Strategy

### Rust/WASM core

- `cargo test -p <new-core-crate>` for pure logic
- `cargo check -p <new-core-crate> --target wasm32-unknown-unknown`

### Extension packaging

- TypeScript typecheck / bundler build for both `main` and `browser` entrypoints
- extension activation smoke test in VS Code desktop using `--extensionDevelopmentKind=web`
- browser-hosted smoke or integration tests via `@vscode/test-web`

### Behavior validation

- verify ticket list render, filter changes, and open-ticket actions in desktop VS Code
- verify copy-to-clipboard and external ticket-viewer navigation in a remote/Codespaces-safe way
- verify desktop-only features are hidden or clearly explained when unavailable

### Manual validation

- browser validation must use an external Chromium-family browser, not VS Code's integrated browser
- record the browser window or display resolution used for any manual UI verification

## Non-Goals For Phase 1

- Eliminating the JS/TS extension entrypoints entirely
- Porting CDP/Browser Bridge automation into the web extension host
- Preserving every current desktop helper in identical form when the host does not support the required runtime capabilities

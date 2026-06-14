# Ticket-vscode Rust/WASM Port

## Goal

Port `memory-viewers/memory-api/tools/ticket-vscode` to a dual-host Rust/WASM-backed architecture without breaking the existing ticket browsing workflow. Target: thin JS/TS host shell + Rust/WASM core, shipping both `main` (Node/Electron/remote) and `browser` (WebWorker) entrypoints.

## Research Findings

VS Code runtime constraints: the `browser` entry runs in a WebWorker bundled as a single file. `process`, `path`, `fs`, `child_process` are not available. Files go through `vscode.workspace.fs`. Processes cannot be spawned. Remote/Codespaces navigation uses `vscode.env.openExternal`, `asExternalUri`, and `clipboard`.

Current extension mixes portable logic (`src/api.ts`, `src/ticketProvider.ts` filter/group/root logic) with Node-bound behavior (`src/extensionSupport.ts` server start/discovery/browser-binary-detection, `src/browserBridge.ts`/`src/browserBridgeCdp.ts` CDP automation, `src/extensionCommands.ts` filesystem assumptions).

## Target Architecture

Both `main` and `browser` entries call the same Rust/WASM core through a narrow `HostCapabilities` adapter boundary. The core never imports `vscode` or Node modules.

## Module Portability Matrix

Frozen classification — every follow-on ticket must honor these buckets.

Buckets: **Portable** (no VS Code/Node dep; Rust core target), **Host shell** (activation/registration/TreeItem; JS/TS in both entries), **Host-adapted** (all hosts, different implementations behind a capability adapter), **Desktop-only/deferred** (Node/Electron only; gated off in web/virtual).

| Module | Bucket | Port action |
|---|---|---|
| `src/api.ts` | Portable | Move shapes + URL building into Rust core; `fetch` behind `FetchCapability` adapter |
| `src/ticketProvider.ts` filter/group/root logic | Portable | Move into Rust core as deterministic tree-model derivation |
| `src/ticketProvider.ts` on-disk folder browsing | Host-adapted | Replace `fs.readdirSync`/`path.join` with `WorkspaceFsCapability` (`vscode.workspace.fs` + `Uri`) |
| `src/ticketTreeItems.ts` | Host shell | Keep in JS shell; replace `Uri.file` with workspace-FS URIs from adapter |
| `src/extensionSupport.ts` config | Host shell | Keep; expose config snapshot to core as plain data |
| `src/extensionSupport.ts` workspace discovery | Host-adapted | HTTP workspace enumeration portable; on-disk `.ticket` scan becomes `WorkspaceFsCapability` |
| `src/extensionSupport.ts` server start | Desktop-only/deferred | Gate behind `ServerControlCapability`; absent in web/virtual |
| `src/extensionSupport.ts` browser binary discovery | Desktop-only/deferred | Drop in web; replace intent with `vscode.env.openExternal` |
| `src/extensionSupport.ts` open external | Host-adapted | Canonical viewer-navigation via `asExternalUri` for all hosts |
| `src/browserBridge.ts` + `src/browserBridgeCdp.ts` | Desktop-only/deferred | Not loaded in web/virtual hosts |
| `src/extensionCommands.ts` | Host shell + adapted | Keep command registration; route file/process/intent through capability adapters |
| `src/extension.ts` | Host shell | Split into shared activation core + `main`/`browser` entrypoints; server orchestration behind `ServerControlCapability` |

Command classification: **Portable/all hosts**: `refresh`, `setSearchQuery`, `setStateFilter`, `clearFilters`, `copyId`, `selectWorkspace`, `openInTicketViewer`, `openBrowser`. **Host-adapted**: `createTicket`, `editTitle`, `setState`, `editDescription`, `previewDescription`, `closeTicket`, `cancelTicket`, `undoTicket`, `addDependency`, `deleteTicket`. **Desktop-only/deferred** (hidden or no-op when capability absent): `startServer`, `bridgeNavigate`, `bridgeConnectCdp`, `bridgeStatus`.

## Host Capability Contract

The seam between JS/TS host shell and Rust/WASM core. Core receives a nullable `HostCapabilities` object.

| Capability | Purpose | Desktop | Remote | Browser/virtual |
|---|---|---|---|---|
| `FetchCapability` | HTTP ticket API and workspace enumeration | global `fetch` | global `fetch` + `asExternalUri` | global `fetch` (CORS-bound) |
| `WorkspaceDiscoveryCapability` | List ticket workspaces | HTTP + FS scan | HTTP + `workspace.fs` | HTTP only |
| `WorkspaceFsCapability` | Read/list ticket files via `readDirectory`/`readFile`/`stat` | full | full | best-effort |
| `ClipboardCapability` | Copy ticket ID/URL via `env.clipboard.writeText` | yes | yes | yes |
| `ExternalUrlCapability` | Open viewer/URLs via `openExternal(asExternalUri(uri))` | yes | yes (port-forwarded) | yes (new tab) |
| `NotificationCapability` | Info/warn/error and prompts | yes | yes | yes |
| `HostDetectionCapability` | `{ uiKind, extensionKind, remoteName, isVirtualWorkspace, isTrusted }` | `desktop-node` | `remote-workspace` | `browser-web`/`virtual` |
| `ServerControlCapability` (optional) | Start/connect a local ticket-viewer server | present | present (remote) | **absent** |
| `BrowserBridgeCapability` (optional) | CDP/Simple-Browser automation | opt-in | absent | **absent** |

Contract rules (frozen):
1. Core depends only on required capabilities. Optional capabilities are nullable; core derives feature availability from their presence and emits gate decisions the shell renders.
2. Host detection uses `env.uiKind`, `extension.extensionKind`, `env.remoteName`, `workspace.isVirtualWorkspace` — never `process` or `navigator` sniffing.
3. All filesystem access goes through `WorkspaceFsCapability`. No core or shared-shell code may import `node:fs`/`node:path`. `vscode.Uri.file(...)` replaced by workspace-folder URIs.
4. Viewer navigation always routes through `ExternalUrlCapability` using `asExternalUri`.
5. Absent capability resolves to `undefined`; core treats it as "feature unavailable"; shell hides/disables the corresponding command/menu.

## Per-Host Behavior Differences

### Server startup
- Desktop: `ServerControlCapability` spawns binary via `child_process`, parses ports, remembers URL.
- Remote: Server runs on remote; client URLs wrapped with `asExternalUri`.
- Browser/virtual: `ServerControlCapability` absent. No spawn. Extension consumes an already-running HTTP server. `startServer` hidden. No server reachable shows "configure a server URL" info node.

### Viewer navigation
- Desktop/Remote: `vscode.env.openExternal(asExternalUri(uri))`. Remote: `asExternalUri` performs port forwarding.
- Browser/virtual: `openExternal` opens new tab. No Simple Browser/CDP path.

### File browsing
- Desktop/Remote: `WorkspaceFsCapability` over `vscode.workspace.fs` replaces `fs.readdirSync`/`path.join`. TreeItem `resourceUri` uses folder URI not `Uri.file`.
- Browser/virtual: `workspace.fs` may be empty. File-child nodes best-effort: render only when `readDirectory` succeeds; otherwise collapse to API-derived data. No `node:fs` fallback.

### Browser-bridge
- Desktop: Optional opt-in, requires `--remote-debugging-port`.
- Remote/Browser/virtual: Not available. `bridge*` commands hidden. Explicit Phase-1 non-goal.

### Host detection summary

| Signal | Desktop node | Remote workspace | Browser/web | Virtual |
|---|---|---|---|---|
| `env.uiKind` | Desktop | Desktop | Web | Web/Desktop |
| `extension.extensionKind` | ui | workspace | web | web |
| `env.remoteName` | undefined | set | undefined | undefined |
| `workspace.isVirtualWorkspace` | false | false | maybe | true |
| `ServerControlCapability` | present | present | absent | absent |
| `BrowserBridgeCapability` | opt-in | absent | absent | absent |

## Loader Constraints (Spike Findings — ticket 14047b99)

### Shared WASM loader

Both `main` and `browser` entries load the `.wasm` binary via `vscode.workspace.fs.readFile` against the extension URI (works in both Node and web hosts):

```ts
const wasmUri = vscode.Uri.joinPath(context.extensionUri, 'out', 'ticket_vscode_core_bg.wasm');
const wasmBytes = await vscode.workspace.fs.readFile(wasmUri);
```

### Desktop `main` entry

CommonJS (`"module": "commonjs"`). wasm-pack target `--target bundler`. `.wasm` copied to `out/` as a separate asset.

### Browser `browser` entry

Single-file bundle (no `require`, no dynamic external `import()`). `"module": "esnext"` in `tsconfig.browser.json`. esbuild bundles `src/extension.browser.ts` to `out/extension.browser.js`. `.wasm` stays as `out/ticket_vscode_core_bg.wasm` — not inlined.

### VSIX packaging

`.vscodeignore` must not exclude `out/*.wasm`. Both entries share the same WASM asset.

### Build pipeline

```
wasm-pack build crates/ticket-vscode-core --target bundler --out-dir tools/ticket-vscode/pkg/
npm run compile          # tsc -> out/extension.js  (main entry)
npm run bundle:browser   # esbuild src/extension.browser.ts -> out/extension.browser.js
```

## Planned Work Track

1. Write the target architecture spec and feature matrix.
2. Prove the extension can load a Rust/WASM module from both desktop and web extension hosts.
3. Extract portable domain logic into a new Rust crate compiled to wasm32.
4. Introduce host capability adapters and redesign Node-only behaviors for web/remote compatibility.
5. Add dual-host packaging, bundling, and test harnesses.
6. Validate parity across desktop, web, and remote-oriented scenarios.

## Validation Strategy

- `cargo test -p ticket-vscode-core` and `cargo check -p ticket-vscode-core --target wasm32-unknown-unknown` for the Rust core.
- TypeScript typecheck and esbuild bundle for both `main` and `browser` entrypoints.
- Extension activation smoke test with `--extensionDevelopmentKind=web`.
- Browser-hosted tests via `@vscode/test-web`.
- Manual: external Chromium-family browser for browser-host path; record window/display resolution.

## Non-Goals For Phase 1

- Eliminating the JS/TS extension entrypoints entirely.
- Porting CDP/Browser Bridge automation into the web extension host.
- Preserving every current desktop helper in identical form when the host does not support the required runtime capabilities.

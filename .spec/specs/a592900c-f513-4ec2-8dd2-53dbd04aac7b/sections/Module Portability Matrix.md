This matrix is the frozen classification of the current `tools/ticket-vscode/src` surface. Every follow-on implementation ticket must honor these buckets rather than re-deciding them locally.

Buckets:
- **Portable** — deterministic logic with no VS Code / Node dependency; target for the Rust/WASM core.
- **Host shell** — VS Code activation/registration/TreeItem wiring; stays in JS/TS, runs in both `main` and `browser` entries.
- **Host-adapted** — behavior that must exist in all hosts but through different implementations behind a capability adapter.
- **Desktop-only / deferred** — depends on Node/Electron runtime capabilities the browser host cannot provide; gated off (not loaded) in web/virtual hosts.

| Module | Current responsibility | Node/Electron coupling today | Bucket | Port action |
|---|---|---|---|---|
| `src/api.ts` | Typed HTTP client for ticket-viewer REST API | `fetch` (web-global, portable) | Portable | Move request/response shapes + URL building into Rust core; keep `fetch` invocation behind the `FetchCapability` adapter |
| `src/ticketProvider.ts` (filter/group/root logic) | Normalization, filtering, grouping, root-ticket detection, tree derivation | none (pure) | Portable | Move into Rust core as deterministic tree-model derivation |
| `src/ticketProvider.ts` (on-disk folder browsing) | `fs.readdirSync` + `path.join` to list ticket files under each node | `node:fs`, `node:path` | Host-adapted | Replace with `WorkspaceFsCapability` (`vscode.workspace.fs` + `Uri`); desktop/remote real, web best-effort/empty |
| `src/ticketTreeItems.ts` | TreeItem subclasses for groups/tickets/files/folders | `path.basename` (string-only); `vscode.Uri.file` (local FS only) | Host shell | Keep in JS shell; replace `Uri.file` with workspace-FS URIs from the adapter so virtual/web workspaces resolve; `basename` can move to core or stay |
| `src/extensionSupport.ts` (config) | `readConfig` from `vscode.workspace.getConfiguration` | none (VS Code API) | Host shell | Keep in shell; expose config snapshot to the core as plain data |
| `src/extensionSupport.ts` (workspace discovery) | `.ticket` discovery via `fs/path`, server `/api/workspaces` probing | `node:fs`, `node:path` | Host-adapted | Split: HTTP workspace enumeration is portable; on-disk `.ticket` discovery becomes `WorkspaceFsCapability` |
| `src/extensionSupport.ts` (server start) | `child_process.spawn` of the ticket-viewer binary, port parsing | `node:child_process`, `process.env` | Desktop-only / deferred | Gate behind `ServerControlCapability`; absent in web/virtual hosts |
| `src/extensionSupport.ts` (browser binary discovery) | Locate Chromium via `process.env` PATH/PROGRAMFILES | `process.env` | Desktop-only / deferred | Drop in web; replace user-facing intent with `vscode.env.openExternal` |
| `src/extensionSupport.ts` (open external) | `vscode.env.openExternal` | none (remote-safe) | Host-adapted | Promote as the canonical viewer-navigation path for all hosts |
| `src/browserBridge.ts` | Local HTTP control server + Simple Browser control + Playwright-over-CDP | `node:http`, `node:net`, CDP | Desktop-only / deferred | Not loaded in web/virtual hosts; desktop-only optional feature |
| `src/browserBridgeCdp.ts` | CDP client | CDP/websocket to Electron | Desktop-only / deferred | Same as `browserBridge.ts` |
| `src/extensionCommands.ts` | VS Code command registration + handlers | `node:fs`, `node:path`, `ChildProcess` types | Host shell + adapted | Keep command registration in shell; route file/process/intent work through capability adapters and core intent derivation |
| `src/extension.ts` | `activate`/`deactivate`, server discovery/start orchestration | `node:child_process` type import | Host shell | Split into shared activation core + `main`/`browser` entrypoints; server orchestration moves behind `ServerControlCapability` |

Command-level classification (from `package.json` `contributes.commands`):

- **Portable / all hosts**: `refresh`, `setSearchQuery`, `setStateFilter`, `clearFilters`, `copyId`, `selectWorkspace`, `openInTicketViewer`, `openBrowser` (via `openExternal`).
- **Host-adapted**: `createTicket`, `editTitle`, `setState`, `editDescription`, `previewDescription`, `closeTicket`, `cancelTicket`, `undoTicket`, `addDependency`, `deleteTicket` (REST-backed, portable intent; confirm/prompt UX stays in shell), plus ticket-file open actions (workspace-FS URIs).
- **Desktop-only / deferred**: `startServer`, `bridgeNavigate`, `bridgeConnectCdp`, `bridgeStatus`. These must be hidden or no-op with a clear message when the host lacks the capability.

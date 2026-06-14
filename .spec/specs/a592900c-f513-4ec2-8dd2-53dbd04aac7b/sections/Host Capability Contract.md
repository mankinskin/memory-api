The capability contract is the single seam between the JS/TS host shell and the Rust/WASM core. The core never imports `vscode` or Node modules; it receives a `HostCapabilities` object whose methods the shell implements per host. Each capability is independently feature-detectable so the core can gate behavior instead of assuming a runtime.

Required capabilities:

| Capability | Purpose | Shape (host-implemented) | Desktop (node) | Remote workspace | Browser / virtual |
|---|---|---|---|---|---|
| `FetchCapability` | Fetch ticket API data and enumerate workspaces over HTTP | `fetch(url, init) -> Response` | global `fetch` | global `fetch` (via `asExternalUri` when targeting localhost) | global `fetch` (CORS-bound) |
| `WorkspaceDiscoveryCapability` | List ticket workspaces | HTTP `/api/workspaces` + optional FS `.ticket` scan | both | HTTP; FS via `workspace.fs` | HTTP only |
| `WorkspaceFsCapability` | Read/list ticket files through URIs | `readDirectory(uri)`, `readFile(uri)`, `stat(uri)` over `vscode.workspace.fs` | full | full | best-effort; may be empty for virtual FS |
| `ClipboardCapability` | Copy ticket ID / URL | `vscode.env.clipboard.writeText` | yes | yes | yes |
| `ExternalUrlCapability` | Open ticket-viewer / external URLs | `vscode.env.openExternal(asExternalUri(uri))` | yes | yes (port-forwarded) | yes (new tab) |
| `NotificationCapability` | Surface info/warn/error and prompts | `window.showInformation/Warning/Error`, `showInputBox`, `showQuickPick` | yes | yes | yes |
| `HostDetectionCapability` | Report host mode for feature gating | `{ uiKind, extensionKind, remoteName, isVirtualWorkspace, isTrusted }` | `desktop-node` | `remote-workspace` | `browser-web` / `virtual` |
| `ServerControlCapability` (optional) | Start/connect a local ticket-viewer server | `start(config)`, `discover()`, `stop()` | present | present (runs in remote) | **absent** |
| `BrowserBridgeCapability` (optional) | CDP/Simple-Browser automation | bridge control surface | present (opt-in) | absent | **absent** |

Contract rules (frozen):

1. The Rust/WASM core depends only on the **required** capabilities. Optional capabilities (`ServerControlCapability`, `BrowserBridgeCapability`) are passed as nullable; the core derives feature availability from their presence plus `HostDetectionCapability`, and emits feature-gate decisions the shell renders.
2. Host detection is derived from `vscode.env.uiKind` (`Desktop` vs `Web`), `context.extension.extensionKind` (`ui` vs `workspace`), `vscode.env.remoteName`, and `vscode.workspace.isVirtualWorkspace` — never from `process` or `navigator` sniffing.
3. All filesystem access goes through `WorkspaceFsCapability` (URI-based). No core or shared-shell code may import `node:fs`/`node:path`. `vscode.Uri.file(...)` is replaced by URIs derived from workspace folders so virtual/web workspaces resolve.
4. Viewer navigation always routes through `ExternalUrlCapability` using `asExternalUri`, making it remote/Codespaces-safe by default.
5. Any capability a host cannot provide resolves to `undefined`; the core must treat that as "feature unavailable" and the shell must hide or disable the corresponding command/menu contribution.

How the four behavior areas that currently assume desktop Node differ per host. This is the frozen reference implementation tickets must follow.

### Server startup
- **Desktop (node):** `ServerControlCapability` may `child_process.spawn` the ticket-viewer binary (current `extensionSupport.startServerTask`), parse listening ports, and remember the URL. `autoStartServer` honored.
- **Remote workspace:** Server runs on the remote; spawn happens in the remote extension host. Client URLs must be wrapped with `asExternalUri` before display/navigation.
- **Browser / virtual:** `ServerControlCapability` is **absent**. No process spawn, no binary discovery, no port parsing. The extension only consumes an already-running server reachable over HTTP; `startServer` command is hidden. If no server URL is configured/reachable, the tree shows a clear "configure a server URL" info node.

### Viewer navigation (open-in-ticket-viewer / open-browser)
- **Desktop:** `vscode.env.openExternal(asExternalUri(uri))`. Current direct `openExternal(Uri.parse(url))` is upgraded to go through `asExternalUri` for consistency.
- **Remote:** `asExternalUri` performs port forwarding so the externally opened URL resolves on the user's machine.
- **Browser / virtual:** `openExternal` opens a new browser tab. No Simple Browser / CDP path. This is the only supported navigation mechanism.

### File browsing (ticket files under a ticket node)
- **Desktop:** `WorkspaceFsCapability` over `vscode.workspace.fs` lists/reads the real on-disk ticket folder (replaces current `fs.readdirSync`/`path.join` in `ticketProvider`). TreeItem `resourceUri` uses the folder URI instead of `Uri.file` so it opens correctly.
- **Remote:** Same API; `workspace.fs` resolves against the remote filesystem transparently.
- **Browser / virtual:** `workspace.fs` may be a virtual provider or empty. File-child nodes are best-effort: render them only when `readDirectory` succeeds; otherwise collapse the ticket node to API-derived data with no file children. No `node:fs` fallback exists.

### Browser-bridge behavior (CDP automation / control server)
- **Desktop:** `BrowserBridgeCapability` is optional and opt-in. Requires VS Code launched with `--remote-debugging-port`. Provides the local HTTP control server + Playwright-over-CDP automation (current `browserBridge.ts` / `browserBridgeCdp.ts`).
- **Remote:** Not available — the CDP target is the local Electron host, which is not reachable from the remote extension host. Bridge commands hidden.
- **Browser / virtual:** Not available and not bundled. `bridgeNavigate`/`bridgeConnectCdp`/`bridgeStatus` commands are hidden. This is an explicit Phase-1 non-goal.

### Host detection summary

| Signal | Desktop node | Remote workspace | Browser/web | Virtual |
|---|---|---|---|---|
| `env.uiKind` | Desktop | Desktop | Web | Web/Desktop |
| `extension.extensionKind` | ui | workspace | web | web |
| `env.remoteName` | undefined | set (e.g. `ssh-remote`, `codespaces`) | undefined | undefined |
| `workspace.isVirtualWorkspace` | false | false | maybe | true |
| `ServerControlCapability` | present | present | absent | absent |
| `BrowserBridgeCapability` | opt-in | absent | absent | absent |

All settings are under the `ticketViewer` namespace. Read on activation via `vscode.workspace.getConfiguration('ticketViewer')`.

| Setting | Type | Default | Effect |
|---|---|---|---|
| `ticketViewer.serverUrl` | `string` | `"http://localhost:3002"` | URL of an externally-managed `ticket-viewer` server. Only used when `autoStartServer` is `false`. |
| `ticketViewer.workspace` | `string` | `""` | Explicit workspace name. When non-empty, overrides workspace auto-detection entirely. |
| `ticketViewer.autoRefreshSeconds` | `number` | `30` | Tree refresh interval in seconds. `0` disables auto-refresh. Minimum enforced by VS Code schema: `0`. |
| `ticketViewer.autoStartServer` | `boolean` | `true` | Spawn `ticket-viewer` binary on activation. When `true`, a random free port is always used (ignores `serverUrl`). |
| `ticketViewer.bridgePort` | `number` | `0` | Port for the Browser Bridge HTTP control server. `0` = auto-assign. |
| `ticketViewer.cdpPort` | `number` | `0` | Chrome DevTools Protocol port. `0` = auto-discover by probing `[9222, 9223, 9229, 9230]`. |
| `ticketViewer.autoConnectCdp` | `boolean` | `true` | Attempt CDP auto-connect on startup. Requires `--remote-debugging-port` when launching VS Code. |
| `ticketViewer.serverBinaryPath` | `string` | `""` | Absolute path to the `ticket-viewer` binary. Empty = auto-detect: checks `target/debug/` inside the workspace folder first, then `PATH`. |
| `ticketViewer.serverWorkingDirectory` | `string` | `""` | Working directory for the server process. Empty = first VS Code workspace folder containing a `.ticket/` directory. |

## Configuration Change Handling

`extension.ts` subscribes to `vscode.workspace.onDidChangeConfiguration`. When `ticketViewer.*` changes:
- If `autoStartServer` is now `false`, `serverUrl` reverts to the configured value.
- Workspace is re-resolved via `resolveActiveWorkspace()`.
- The provider is updated with the new `serverUrl`, `workspace`, and `autoRefreshSeconds`.
- The status bar tooltip is updated.

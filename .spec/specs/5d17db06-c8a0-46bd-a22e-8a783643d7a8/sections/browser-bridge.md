The `BrowserBridge` class (`browserBridge.ts`) provides two capabilities:

1. **Simple Browser navigation** — drives VS Code's built-in `simpleBrowser.show` command to display the ticket-viewer SPA inside VS Code.
2. **Playwright/CDP page automation** — optionally connects to VS Code's Electron renderer via Chrome DevTools Protocol to perform programmatic page interactions (click, fill, screenshot, evaluate, accessibility snapshot).

## Control Server

`BrowserBridge.start()` binds a plain Node.js `http.Server` on `127.0.0.1` at the configured `bridgePort` (0 = auto-assign). Only localhost connections are accepted. CORS headers are set to `Access-Control-Allow-Origin: *` for local developer tooling convenience; the server is never exposed on a network interface.

### Endpoints

| Method | Path | Body | Description |
|---|---|---|---|
| `GET` | `/status` | — | Returns `BridgeState` (controlPort, cdpConnected, currentUrl) |
| `POST` | `/navigate` | `{ url: string }` | Opens URL in Simple Browser; optionally re-targets CDP page |
| `POST` | `/connect-cdp` | — | Establishes CDP connection |
| `POST` | `/click` | `{ selector: string }` | Clicks a CSS selector on the CDP-connected page |
| `POST` | `/fill` | `{ selector: string, value: string }` | Fills an input |
| `POST` | `/screenshot` | — | Returns `image/png` binary |
| `POST` | `/snapshot` | — | Returns accessibility tree JSON (falls back to page HTML) |
| `POST` | `/evaluate` | `{ expression: string }` | Evaluates JS in page context |
| `GET` | `/pages` | — | Lists `{ url, title }` for all CDP-visible pages |
| `POST` | `/close` | — | Disconnects CDP; clears currentUrl |

Request body size is limited to 1 MB.

## CDP Auto-Connect

On startup, if `ticketViewer.autoConnectCdp` is true, `_autoConnectCdp()` probes ports `[9222, 9223, 9229, 9230]` via `GET /json/version` (1.5 s timeout each). The first reachable port is used. Probe failures are silent (logged to "Browser Bridge" output channel only).

To enable CDP, launch VS Code with `--remote-debugging-port=9222`.

## Playwright Loading

`playwright` is loaded via a dynamic `require('playwright')` at the moment `connectCdp()` is first called. This keeps the extension activatable even when the package is not installed. If the require fails, a warning notification is shown (unless in silent/auto-connect mode).

## Security Considerations

- The control server binds only to `127.0.0.1`; it is not reachable from other hosts.
- The `POST /evaluate` endpoint executes arbitrary JavaScript in the browser page context. This is intentional for automation use cases (MCP tools, test scripts) but must not be exposed beyond localhost.
- The accessibility snapshot and screenshot endpoints expose page content; callers must be trusted local processes.
- CDP auto-connect probes well-known ports; if VS Code is not running with `--remote-debugging-port`, no connection is attempted and no data is exposed.

The `ticket-vscode` extension (VS Code package id: `ticket-viewer`, v0.1.0) surfaces the ticket graph from a running `ticket-viewer` Axum/Dioxus server directly inside VS Code's activity bar. It allows developers to browse, create, and manage tickets without switching to a browser or terminal.

## Technology Stack

- **Runtime**: VS Code Extension Host (Node.js)
- **Language**: TypeScript 5.x
- **VS Code API**: `^1.90.0`
- **Automation**: Playwright (`^1.58.2`) — optional, used only for CDP-based page automation
- **Build**: `tsc` (no bundler); outputs to `out/`
- **Tests**: Jest + ts-jest (unit tests only; no e2e activation tests)

## Activation

The extension activates unconditionally at VS Code startup (`onStartupFinished`). On activation it:

1. Optionally auto-spawns the `ticket-viewer` binary (port-0 auto-assign; reads `TICKET_VIEWER_PORT=<n>` from stdout to discover the actual port).
2. Resolves the active ticket workspace (explicit config > `.ticket/` directory scan > server API fallback).
3. Registers the `TicketTreeProvider` and attaches it to the `ticket-viewer.tickets` TreeView.
4. Starts the `BrowserBridge` control server.
5. Creates a status bar item showing the workspace name and live ticket counts.

On deactivation, the spawned server process is killed.

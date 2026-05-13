The `ticket-vscode` extension is a VS Code sidebar integration for the ticket system. It connects to a `ticket-viewer` HTTP server, renders the ticket graph in a TreeView grouped by state, and exposes a full command palette of ticket CRUD operations — all without leaving VS Code.

### Design Goals

- Zero-friction: auto-start the server, auto-detect the workspace, auto-refresh the tree.
- Mirror the ticket CLI's capability surface in a GUI idiom (QuickPick, InputBox, confirmations).
- Keep the extension thin: it is an HTTP client wrapper over the `ticket-viewer` API; all business logic lives in `ticket-api`.
- Provide an escape hatch to the Dioxus SPA via VS Code's Simple Browser when richer UI is needed.

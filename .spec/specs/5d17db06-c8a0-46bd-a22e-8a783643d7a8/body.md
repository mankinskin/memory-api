The `ticket-vscode` extension (VS Code package id: `ticket-viewer`, v0.1.0) surfaces the ticket graph from a running `ticket-viewer` Axum/Dioxus server directly inside VS Code's activity bar. It allows developers to browse and manage tickets inside the editor while opening full ticket detail routes in the ticket viewer when deeper navigation is needed.

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

## Traceability

- Tracking ticket: `.ticket/tickets/5b330dd5-2dcc-4460-b468-43ff4c35bfba`
- Updated documentation: `.spec/specs/5d17db06-c8a0-46bd-a22e-8a783643d7a8/sections/commands.md`
- Updated implementation: `tools/ticket-vscode/src/extensionCommands.ts`, `tools/ticket-vscode/package.json`
- Focused regression tests: `tools/ticket-vscode/test/unit/extensionCommands.test.ts`, `tools/ticket-vscode/test/unit/packageContributions.test.ts`

## Validation

- Passed: `npm run test:unit -- --runInBand --runTestsByPath test/unit/extensionCommands.test.ts test/unit/packageContributions.test.ts`
- Passed: `npm run compile`
- Passed after refreshing spec code refs: `spec.exe refs 5d17db06 validate --json`

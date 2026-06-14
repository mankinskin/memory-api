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
	When the server exposes canonical workspace ids that differ from VS Code folder names, the extension must map the detected local `.ticket` root to the matching server workspace by label or path and otherwise prefer the server-declared active workspace over the first returned workspace.
	When auto-start races server readiness, the extension must re-resolve the active workspace after the server becomes reachable before relying on the provider's durable workspace binding for ticket listing.
3. Registers the `TicketTreeProvider` and attaches it to the `ticket-viewer.tickets` TreeView.
4. Starts the `BrowserBridge` control server.
5. Creates a status bar item showing the workspace name and live ticket counts.

On deactivation, the spawned server process is killed.

## Traceability

- Tracking tickets: `.ticket/tickets/5b330dd5-2dcc-4460-b468-43ff4c35bfba`, `.ticket/tickets/46d16755-309b-479f-aab2-624c3fa7ce9b`, `.ticket/tickets/ff2872ad-74be-4e5d-a7ba-416c73506252`
- Updated documentation: `.spec/specs/5d17db06-c8a0-46bd-a22e-8a783643d7a8/body.md`, `.spec/specs/5d17db06-c8a0-46bd-a22e-8a783643d7a8/sections/commands.md`
- Updated implementation: `tools/ticket-vscode/src/api.ts`, `tools/ticket-vscode/src/extension.ts`, `tools/ticket-vscode/src/extensionCommands.ts`, `tools/ticket-vscode/src/extensionSupport.ts`
- Focused regression tests: `tools/ticket-vscode/test/unit/extensionCommands.test.ts`, `tools/ticket-vscode/test/unit/packageContributions.test.ts`, `tools/ticket-vscode/test/unit/resolveServerLaunch.test.ts`

## Validation

- Passed: `npm run test:unit -- --runInBand --runTestsByPath test/unit/extensionCommands.test.ts test/unit/packageContributions.test.ts`
- Passed: `npm run test:unit -- --runInBand --runTestsByPath test/unit/resolveServerLaunch.test.ts`
- Passed: `npm run compile`
- Passed after refreshing spec code refs: `spec.exe refs 5d17db06 validate --json`

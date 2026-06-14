# [ticket-vscode] Replace Node-bound behaviors with host capability adapters

Refactor the extension host layer so runtime-specific behavior is isolated behind explicit capabilities instead of being embedded throughout `extensionSupport.ts`, `extensionCommands.ts`, and `ticketProvider.ts`.

Acceptance criteria:
- [x] Runtime-sensitive behavior is moved behind capability adapters or split host-specific modules.
- [x] File and folder browsing uses VS Code URI/workspace APIs where supported and defines fallback behavior for virtual workspaces.
- [x] `openExternal`, `asExternalUri`, and `env.clipboard` replace local-process assumptions where appropriate for remote/browser safety.
- [x] Commands or features that remain desktop-only are hidden, gated, or explained explicitly instead of failing at runtime.

## Implementation summary

### New files

- `src/hostCapabilities.ts` — The complete frozen `HostCapabilities` contract (spec a592900c "Host Capability Contract"): all 7 required + 2 optional capability interfaces, `HostKind` type, `detectHostKind()` helper, and concrete implementations (`VsCodeClipboardCapability`, `VsCodeExternalUrlCapability`, `VsCodeNotificationCapability`, `VsCodeWorkspaceFsCapability`, `VsCodeHostDetectionCapability`, `GlobalFetchCapability`).
- `src/workspaceDiscovery.ts` — `DesktopWorkspaceDiscovery` (Node-only: `node:fs` + `node:path` confined here) and `HttpOnlyWorkspaceDiscovery` (browser-safe, HTTP only). Node modules are isolated in one file, not scattered.

### Modified files

- `src/ticketProvider.ts` — removed `node:fs`/`node:path` imports. `_ticketsDir: string` replaced with `_ticketsDirUri: vscode.Uri | undefined` + `_workspaceFs: WorkspaceFsCapability | undefined`. `_getTicketFolderChildren` and directory reads replaced with async `_readDirEntriesAsync` via `WorkspaceFsCapability`. Constructor and `update()` updated accordingly.
- `src/extensionSupport.ts` — `openTicketViewer()` upgraded: both the no-binary-found path and the error fallback now go through `asExternalUri` before `openExternal` (rule 4). Added `resolveTicketsDirUri()` wrapper returning `vscode.Uri | undefined`.
- `src/extensionCommands.ts`, `src/extension.ts` — updated to call `resolveTicketsDirUri` instead of `resolveTicketsDir`.
- Test mocks updated to match new `resolveTicketsDirUri` + `Uri`-shaped argument.

Results: `npm run compile` — clean. `node scripts/bundle-browser.mjs` — 2.9 kB. `npm run test:unit` — 32/32 passed.

## Frozen architecture boundary

Spec `ticket-vscode/rust-wasm-port` (a592900c, state `reviewed`) — Host Capability Contract and Per-Host Behavior Differences sections.

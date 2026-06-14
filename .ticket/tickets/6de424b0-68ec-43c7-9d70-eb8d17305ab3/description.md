# [ticket-vscode] Validate Rust/WASM parity across desktop, web, and remote hosts

This ticket closes the track by validating the implemented port against the spec and the current user-visible workflows.

Core workflows to validate:
- render the ticket tree
- search/filter tickets
- open the selected ticket in ticket-viewer
- copy the selected ticket ID to the clipboard
- file-browsing and desktop-only helpers, where still supported

## Dependency links

- Tracker: [6d07d610 Rust/WASM port track](../6d07d610-75c1-448a-afd5-6ae15098ca21/ticket.toml)
- Depends on: [362448d4 Add dual-host packaging, bundling, and extension test harnesses](../362448d4-ccf1-4b9d-90f3-d4577da83a65/ticket.toml)
- Final validation ticket in the execution chain.

Acceptance criteria:
- [ ] Validation results are recorded for desktop/local, browser/web, and at least one remote-oriented host scenario or documented equivalent.
- [ ] External Chromium-family manual validation is captured for the browser-facing path, including the window or display resolution used.
- [ ] Desktop-only or unsupported web-host features are explicitly documented and linked back to the spec.
- [ ] Spec traceability and ticket descriptions include the exact validation commands, outcomes, and any remaining blockers.

## Validation results so far (2026-06-15)

### Rust core validation

Passed:
- `cargo test -p ticket-vscode-core`
  - result: 16 passed, 0 failed
- `cargo check -p ticket-vscode-core --target wasm32-unknown-unknown --features wasm`
  - result: passed

### Extension build + packaging validation

Passed:
- `cd memory-viewers/memory-api/tools/ticket-vscode && npm run build`
  - result: `tsc -p tsconfig.json` passed; `node ./scripts/bundle-browser.mjs` produced `out/extension.browser.js`
- `cd memory-viewers/memory-api/tools/ticket-vscode && npm run test:unit`
  - result: 32 passed, 0 failed
- `cd memory-viewers/memory-api/tools/ticket-vscode && npm run package`
  - result: VSIX packaged successfully

Packaging fixes made during validation:
- corrected `build:wasm` path so `wasm-pack` writes into the extension-local `pkg/` directory instead of the Rust crate directory
- updated `package` script to run `build:full` so wasm assets are always built before packaging
- tightened `.vscodeignore` so `out/__mocks__/` is excluded and `pkg/` contents are included

Verified VSIX contents now include:
- `pkg/ticket_vscode_core.js`
- `pkg/ticket_vscode_core_bg.js`
- `pkg/ticket_vscode_core_bg.wasm`
- `pkg/ticket_vscode_core_bg.wasm.d.ts`

### Browser / remote validation blockers

Not yet validated:
- no `@vscode/test-web` harness or `test:web` script exists in `tools/ticket-vscode/package.json`
- no external Chromium-family manual validation has been captured yet
- no remote-oriented validation run has been captured yet

### Additional parity finding

`package.json` still contributes desktop-only commands unconditionally:
- `ticket-viewer.startServer`
- `ticket-viewer.bridgeNavigate`
- `ticket-viewer.bridgeConnectCdp`
- `ticket-viewer.bridgeStatus`

This means the spec requirement to hide/disable unavailable capabilities in web/virtual hosts is not yet fully evidenced. That needs either:
1. contribution/runtime gating changes, or
2. explicit manual proof that the commands are hidden/disabled in browser/remote hosts.

## Frozen architecture boundary

The Rust/WASM architecture is frozen in spec `ticket-vscode/rust-wasm-port` (a592900c, state `reviewed`). Validate against it directly:
- "Per-Host Behavior Differences" is the expected-behavior oracle per host: server startup, viewer navigation (`asExternalUri`), file browsing (`workspace.fs` best-effort on virtual), and browser-bridge (desktop-only).
- "Host Capability Contract" rule 5: confirm commands whose capability is absent (`startServer`, `bridge*`) are hidden/disabled rather than failing.
- "Validation Strategy": run `cargo test` + `cargo check --target wasm32-unknown-unknown` for the core, and the desktop + `@vscode/test-web` extension harnesses; manual browser validation must use an external Chromium-family browser with the window/display resolution recorded.

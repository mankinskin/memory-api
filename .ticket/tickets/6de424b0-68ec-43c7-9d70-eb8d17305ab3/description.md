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

## Validation evidence (test-cli store)

Validation results are recorded in the native test-result store instead of inline here. Each execution links back to this ticket (`6de424b0-68ec-43c7-9d70-eb8d17305ab3`).

- Store root: `memory-api/.test/default/`
- Specs: `memory-api/.test/default/specs/` (vt-core-tests, vt-core-wasm-check, vt-ext-build, vt-ext-unit, vt-ext-package, vt-browser-web, vt-remote)
- Executions: `memory-api/.test/default/executions/`

Query the evidence linked to this ticket:

```bash
./target/debug/test.exe --store-root "$PWD/memory-api/.test" --toon list --ticket 6de424b0-68ec-43c7-9d70-eb8d17305ab3
```

Recorded outcomes (2026-06-15):

| Execution | Spec | Command | Outcome |
|---|---|---|---|
| exec-vt-core-tests-20260615 | vt-core-tests | `cargo test -p ticket-vscode-core` (16 passed) | passed |
| exec-vt-core-wasm-check-20260615 | vt-core-wasm-check | `cargo check --target wasm32-unknown-unknown -p ticket-vscode-core` | passed |
| exec-vt-ext-build-20260615 | vt-ext-build | `npm run build` (esbuild + wasm bundle) | passed |
| exec-vt-ext-unit-20260615 | vt-ext-unit | `npm run test:unit` (32 passed) | passed |
| exec-vt-ext-package-20260615 | vt-ext-package | `npm run package` (VSIX contains wasm payload) | passed |
| exec-vt-browser-web-20260615 | vt-browser-web | external Chromium-family manual validation | blocked — no web-extension harness available |
| exec-vt-remote-20260615 | vt-remote | remote-host scenario | blocked — no remote-host harness available |

Packaging fixes made during validation:
- corrected `build:wasm` path so `wasm-pack` writes into the extension-local `pkg/` directory instead of the Rust crate directory
- updated `package` script to run `build:full` so wasm assets are always built before packaging
- tightened `.vscodeignore` so `out/__mocks__/` is excluded and `pkg/` contents are included

Verified VSIX contents include `pkg/ticket_vscode_core.js`, `pkg/ticket_vscode_core_bg.js`, `pkg/ticket_vscode_core_bg.wasm`, `pkg/ticket_vscode_core_bg.wasm.d.ts`.

### Remaining blockers

- no `@vscode/test-web` harness or `test:web` script exists in `tools/ticket-vscode/package.json` (vt-browser-web blocked)
- no external Chromium-family manual validation captured yet (vt-browser-web blocked)
- no remote-oriented validation run captured yet (vt-remote blocked)
- `package.json` still contributes desktop-only commands (`ticket-viewer.startServer`, `ticket-viewer.bridgeNavigate`, `ticket-viewer.bridgeConnectCdp`, `ticket-viewer.bridgeStatus`) unconditionally; spec requirement to hide/disable unavailable capabilities in web/virtual hosts is not yet fully evidenced.

## Frozen architecture boundary

The Rust/WASM architecture is frozen in spec `ticket-vscode/rust-wasm-port` (a592900c, state `reviewed`). Validate against it directly:
- "Per-Host Behavior Differences" is the expected-behavior oracle per host: server startup, viewer navigation (`asExternalUri`), file browsing (`workspace.fs` best-effort on virtual), and browser-bridge (desktop-only).
- "Host Capability Contract" rule 5: confirm commands whose capability is absent (`startServer`, `bridge*`) are hidden/disabled rather than failing.
- "Validation Strategy": run `cargo test` + `cargo check --target wasm32-unknown-unknown` for the core, and the desktop + `@vscode/test-web` extension harnesses; manual browser validation must use an external Chromium-family browser with the window/display resolution recorded.

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

## Frozen architecture boundary

The Rust/WASM architecture is frozen in spec `ticket-vscode/rust-wasm-port` (a592900c, state `reviewed`). Validate against it directly:
- "Per-Host Behavior Differences" is the expected-behavior oracle per host: server startup, viewer navigation (`asExternalUri`), file browsing (`workspace.fs` best-effort on virtual), and browser-bridge (desktop-only).
- "Host Capability Contract" rule 5: confirm commands whose capability is absent (`startServer`, `bridge*`) are hidden/disabled rather than failing — covers the "desktop-only features hidden or explained" acceptance criterion.
- "Validation Strategy": run `cargo test` + `cargo check --target wasm32-unknown-unknown` for the core, and the desktop + `@vscode/test-web` extension harnesses; manual browser validation must use an external Chromium-family browser with the window/display resolution recorded.

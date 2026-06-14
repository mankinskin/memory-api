# [ticket-vscode] Freeze Rust/WASM architecture spec and feature matrix

Use the new planning spec `ticket-vscode/rust-wasm-port` as the canonical design surface for the migration.

Goals:
- classify each current module and user-facing command as portable, host-adapted, or desktop-only/deferred
- define the JS host adapter boundary vs the Rust/WASM core boundary
- document phase-1 non-goals so implementation tickets do not assume impossible web-host behavior

## Dependency links

- Tracker: [6d07d610 Rust/WASM port track](../6d07d610-75c1-448a-afd5-6ae15098ca21/ticket.toml)
- Next ticket in sequence: [14047b99 Prove dual-host WASM activation](../14047b99-41d6-4899-bec6-4a919bffcc2d/ticket.toml)
- This ticket defines the architecture boundary that all follow-on tickets depend on.

Acceptance criteria:
- [ ] The spec includes a module-by-module portability matrix for the current extension surface.
- [ ] The spec defines a host capability contract covering fetch, workspace discovery, URI/file access, clipboard, external URLs, notifications, and host detection.
- [ ] The spec documents how desktop, remote, and browser hosts differ for server startup, viewer navigation, file browsing, and browser-bridge behavior.
- [ ] Each follow-on implementation ticket references the agreed architecture boundary instead of re-deciding it locally.

# [ticket-vscode] Validate Rust/WASM parity across desktop, web, and remote hosts

This ticket closes the track by validating the implemented port against the spec and the current user-visible workflows.

Core workflows to validate:
- render the ticket tree
- search/filter tickets
- open the selected ticket in ticket-viewer
- copy the selected ticket ID to the clipboard
- file-browsing and desktop-only helpers, where still supported

Acceptance criteria:
- [ ] Validation results are recorded for desktop/local, browser/web, and at least one remote-oriented host scenario or documented equivalent.
- [ ] External Chromium-family manual validation is captured for the browser-facing path, including the window or display resolution used.
- [ ] Desktop-only or unsupported web-host features are explicitly documented and linked back to the spec.
- [ ] Spec traceability and ticket descriptions include the exact validation commands, outcomes, and any remaining blockers.

# [ticket-vscode] Extract portable Rust core for ticket/domain logic

Move deterministic, serializable logic out of the current TypeScript extension into a new Rust core that is compiled to WASM and driven by host-provided data.

Candidate logic to port first:
- ticket, edge, and schema data models
- state filtering and search filtering
- dependency-root detection and state grouping
- tree-model derivation used by the sidebar view
- ticket URL / command intent derivation that does not require direct VS Code APIs

Acceptance criteria:
- [ ] The Rust core crate contains no direct VS Code API bindings and no Node-specific assumptions.
- [ ] The JS/TS host passes API payloads into the core and receives serializable tree/view-model output.
- [ ] Focused Rust tests cover grouping, filtering, dependency-root logic, and any ported URL derivation.
- [ ] `cargo check --target wasm32-unknown-unknown` passes for the core crate and the extension can render the ticket tree from core-derived results.

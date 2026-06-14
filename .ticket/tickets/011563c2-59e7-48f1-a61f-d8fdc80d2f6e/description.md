# [ticket-vscode] Extract portable Rust core for ticket/domain logic

Move deterministic, serializable logic out of the current TypeScript extension into a new Rust core that is compiled to WASM and driven by host-provided data.

Candidate logic to port first:
- ticket, edge, and schema data models
- state filtering and search filtering
- dependency-root detection and state grouping
- tree-model derivation used by the sidebar view
- ticket URL / command intent derivation that does not require direct VS Code APIs

## Dependency links

- Tracker: [6d07d610 Rust/WASM port track](../6d07d610-75c1-448a-afd5-6ae15098ca21/ticket.toml)
- Depends on: [14047b99 Prove dual-host WASM activation](../14047b99-41d6-4899-bec6-4a919bffcc2d/ticket.toml)
- Unblocks: [bfafde19 Replace Node-bound behaviors with host capability adapters](../bfafde19-ddf7-47ef-966e-a1135be4efd6/ticket.toml)

Acceptance criteria:
- [ ] The Rust core crate contains no direct VS Code API bindings and no Node-specific assumptions.
- [ ] The JS/TS host passes API payloads into the core and receives serializable tree/view-model output.
- [ ] Focused Rust tests cover grouping, filtering, dependency-root logic, and any ported URL derivation.
- [ ] `cargo check --target wasm32-unknown-unknown` passes for the core crate and the extension can render the ticket tree from core-derived results.

## Frozen architecture boundary

The Rust/WASM architecture is frozen in spec `ticket-vscode/rust-wasm-port` (a592900c, state `reviewed`). The "Module Portability Matrix" pins exactly which modules move into this core:
- Portable → core: `api.ts` request/response shapes + URL building; `ticketProvider.ts` filter/group/root-detection/tree-derivation; ticket URL / command intent derivation.
- Stays in the host shell (do NOT pull into the core): TreeItem subclasses (`ticketTreeItems.ts`), command registration, and any `vscode`/Node access.
- "Host Capability Contract" rule 1 + 5: the core receives a `HostCapabilities` object and derives feature-gate decisions from capability presence; missing capability ⇒ "feature unavailable".

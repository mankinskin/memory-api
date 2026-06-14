# [ticket-vscode] Extract portable Rust core for ticket/domain logic

Move deterministic, serializable logic out of the current TypeScript extension into a new Rust core that is compiled to WASM and driven by host-provided data.

Acceptance criteria:
- [x] The Rust core crate contains no direct VS Code API bindings and no Node-specific assumptions.
- [x] The JS/TS host passes API payloads into the core and receives serializable tree/view-model output.
- [x] Focused Rust tests cover grouping, filtering, dependency-root logic, and any ported URL derivation.
- [x] `cargo check --target wasm32-unknown-unknown` passes for the core crate and the extension can render the ticket tree from core-derived results.

## Implementation summary

`memory-viewers/memory-api/crates/ticket-vscode-core/src/lib.rs` expanded from the spike stub to the full domain model:

- **Domain types**: `TicketSummary` (id, type, title, state) and `EdgeRecord` (from, to, kind) — mirror `api.ts` shapes; exposed via `#[wasm_bindgen]` under the `wasm` feature flag.
- **Host-kind gates**: `HostKind` enum + `supports_server_control`, `supports_browser_bridge`, `supports_file_browsing` — pure Rust functions emitting feature-gate decisions used by the JS shell.
- **Filtering**: `ticket_matches(ticket, state_filter, query)` — pure predicate; case-insensitive substring match on title and id.
- **Dependency maps**: `DependencyMaps::build(tickets, edges)` — builds `deps_of` and `parent_of` maps, skipping unknown tickets and non-`depends_on` edges.
- **State grouping + root detection**: `build_state_groups(tickets, edges, state_order, state_filter, query)` — mirrors `buildStateGroups` from `ticketProvider.ts`; respects schema state ordering; identifies root tickets as those with no same-state parent.
- **URL/intent derivation**: `ticket_viewer_url(base_url, workspace, ticket_id)` and `ticket_display_label(id, title)`.

Results: `cargo test -p ticket-vscode-core` — 16/16 passed. `cargo check --target wasm32-unknown-unknown --features wasm` — clean.

## Frozen architecture boundary

Spec `ticket-vscode/rust-wasm-port` (a592900c, state `reviewed`) — Module Portability Matrix: all Portable-bucketed logic is now in the Rust core.

# [ticket-vscode] Move ticket 694d74b4 into the memory-api workspace store

## Goal

Relocate ticket **`694d74b4-028b-4602-8090-d6200d577d4a`** ("[ticket-vscode] Integrate Rust/WASM core into TS hosts and remove replaced legacy logic") from the **root** store into the **memory-api** store so it lives alongside the rest of its track.

## Why

`694d74b4` was created through the repo-root-rooted MCP server, so it physically landed in:

- Current (wrong) location: `context-engine/.ticket/tickets/694d74b4-.../`

It belongs with its tracker and siblings, which all live in:

- Target location: `memory-viewers/memory-api/.ticket/tickets/`

The dependency edges resolve today only because the root store **aggregates** the nested memory-api store as a scan root. Co-locating the ticket removes that asymmetry and keeps the whole `6d07d610` track in one store.

## Blocked on

This is a **reminder / consumer** ticket. Do **not** hand-move the folder — that risks the exact dangling-edge/orphaned-index problems the tooling is being built to prevent.

- Blocked by the cross-workspace move + re-link tooling planned in: **[ticket-api] Cross-workspace move + automatic reference re-linking for store entries** (the planning ticket created in the memory-api store). Use the delivered `ticket move` tool once available.

## References to verify after the move

When the move tool runs, confirm it re-links all of these (UUID stays stable; only path/index rows change):

- Outbound edges of `694d74b4`: `depends_on` → `011563c2`, `bfafde19`, `362448d4-ccf1`.
- Inbound edges into `694d74b4`: tracker `6d07d610` `depends_on` `694d74b4`; validation `6de424b0` `depends_on` `694d74b4`.
- Any board entry/lease referencing `694d74b4`.
- Any spec/test/doc that cites the **old folder path** under `context-engine/.ticket/`.

## Acceptance criteria

- [ ] `694d74b4` resides under `memory-viewers/memory-api/.ticket/tickets/` and is no longer present in the root store.
- [ ] All four+ edges above resolve correctly from the memory-api store with no dangling references in either store.
- [ ] `ticket health` on the `6d07d610` subgraph reports no new dangling-edge or convergence findings caused by the move.
- [ ] The move was performed with the cross-workspace move tool (not a manual folder move).

## Hand-off notes

- UUID `694d74b4-028b-4602-8090-d6200d577d4a` is stable across the move — re-link by ID.
- Run `ticket scan --reindex` (or the tool's equivalent) on both source and target stores after the move and verify search + edges.

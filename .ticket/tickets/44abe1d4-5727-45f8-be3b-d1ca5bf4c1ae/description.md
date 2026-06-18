# [ticket-vscode] Move ticket 694d74b4 into the memory-api workspace store

## Goal

Relocate ticket `694d74b4-028b-4602-8090-d6200d577d4a` ("[ticket-vscode] Integrate Rust/WASM core into TS hosts and remove replaced legacy logic") from the root store into the memory-api store so it lives alongside the rest of its track.

## Current locations

- Current location: `context-engine/.ticket/tickets/694d74b4-.../`
- Target location: `memory-api/.ticket/tickets/`

The dependency edges resolve today only because the root store aggregates the nested memory-api store as a scan root. Co-locating the ticket removes that asymmetry and keeps the whole `6d07d610` track in one store.

## Blocked on

Do **not** hand-move the folder.

This reminder now depends on the execution tracker:

- `505b2cd4` — deliver the safe cross-workspace ticket move tool for git-backed stores

That tracker in turn depends on planning ticket `13e9ce28` and the focused storage/surface/validation work required to move the ticket safely.

## References to verify after the move

- Outbound edges of `694d74b4`: `depends_on` -> `011563c2`, `bfafde19`, `362448d4-ccf1`
- Inbound edges into `694d74b4`: tracker `6d07d610` -> `694d74b4`; validation `6de424b0` -> `694d74b4`
- Any board entry/lease referencing `694d74b4`
- Any spec/test/doc that cites the old folder path under `context-engine/.ticket/`

## Acceptance criteria

- [ ] `694d74b4` resides under `memory-api/.ticket/tickets/` and is no longer present in the root store.
- [ ] The known inbound and outbound edges still resolve correctly from the memory-api store.
- [ ] `ticket health` on the `6d07d610` subgraph reports no new dangling-edge or convergence findings caused by the move.
- [ ] The move was performed with the delivered move tool, not a manual folder move.
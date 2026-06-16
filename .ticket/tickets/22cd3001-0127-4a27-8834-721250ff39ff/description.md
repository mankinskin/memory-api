# [ticket-api] Enforce board safety and migrate historical board rows during ticket moves

## Goal

Handle board-state references safely during a ticket move so live ownership is never silently lost.

## Scope

Implement the chosen v1 board policy:

- **block** the move when the ticket has any active or stale board claim / lease
- migrate historical/inactive board rows needed to preserve audit context into the destination store
- remove migrated source-store rows or mark them as moved so the old store does not retain orphaned board history for the moved ticket
- surface clear diagnostics telling the operator how to clear or hand off active board state before retrying

## Acceptance criteria

- [ ] Active or stale board ownership prevents a move before any file mutation starts.
- [ ] Historical board rows follow the ticket to the destination store.
- [ ] No orphaned board rows remain in the source store after a successful move.

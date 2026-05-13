Add `created_at` field to the `TicketSummary` struct in the ticket-http handler so the frontend can sort tickets by creation date.

## Context

`TicketSummary` in `tools/ticket-http/src/serve/handlers/tickets.rs` currently omits `created_at`, but `IndexedTicket` in the storage layer (`crates/ticket-api/src/storage/indexed.rs`) already has it. This is a 1-line Rust struct addition + 1-line mapping change.

## Changes Required

1. Add `created_at: DateTime<Utc>` (or `String`) to `TicketSummary` struct.
2. Map `indexed.created_at` in the `list_tickets()` handler.
3. Update `ticket-viewer/frontend/src/types.ts` to include `created_at: string` on `TicketSummary`.

## Why

The sortable FileTree ticket (d7971816) depends on this to enable "sort by created date" in ticket-viewer.

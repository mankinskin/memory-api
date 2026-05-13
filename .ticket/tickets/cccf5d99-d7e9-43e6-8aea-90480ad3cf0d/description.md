# Problem

The ticket explorer currently fails to honor the active state filter once the user types a search query.

Root cause: `memory-viewers/memory-api/tools/http/ticket-http/src/serve/handlers/tickets/read.rs` takes a dedicated `query` branch that calls `store.search_tickets(query, limit)` and never applies `params.state`. The ticket-viewer sidebar relies on that endpoint through `memory-viewers/ticket-viewer/frontend/dioxus/src/routes/list/page.rs`, so the UI cannot produce correct combined search + state results.

## Why this matters

- Users see tickets outside the selected state while the state chip looks active.
- Empty-state behavior is misleading because a matching ticket may exist in the requested state, but the response was never filtered correctly.
- Any future viewer flow that combines free-text search with state constraints will inherit the same bug.

## Acceptance Criteria

1. `GET /api/tickets?workspace=<ws>&query=<q>&state=<s>` returns only tickets that match both the query and the requested state.
2. The endpoint applies filtering before the final response `limit` is enforced, so matching tickets are not dropped just because unrelated states ranked earlier.
3. Query-only and state-only behavior continue to work unchanged.
4. Automated coverage exercises query-only, state-only, and combined query + state requests in the ticket HTTP/API layer.
5. The ticket-viewer sidebar shows correct filtered search results when a state chip is active and the filter textbox is non-empty.

## Implementation Notes

- Prefer fixing this at the API/storage boundary instead of patching the sidebar client-side.
- If the existing search API cannot combine predicates efficiently, document the fallback strategy and cover it with tests.
- Verify the sidebar empty state after a combined query returns zero matches.

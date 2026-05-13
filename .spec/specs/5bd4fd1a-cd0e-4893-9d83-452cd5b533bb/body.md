# ticket-http: ticket list endpoint

Canonical contract for the ticket list API consumed by the Dioxus ticket-viewer explorer and related ticket-picking surfaces.

## Endpoint

- `GET /api/tickets`

## Query semantics

- The endpoint accepts an optional free-text query and zero or more state filters through the client contract.
- Query and state filters are conjunctive: when both are present, a ticket must satisfy the text query and the state filter set.
- When more than one state filter is supplied, the state portion is matched with OR semantics.
- Supplying a query must not bypass or discard active state filters.
- Supplying state filters must not change the text-query matching behavior.
- Filtering is applied before any limit or truncation logic.

## Result contract

- Query-only requests return all tickets matching the query.
- State-only requests return all tickets whose state is in the requested state set.
- Combined requests return only tickets that satisfy both conditions.
- Empty result sets are valid and must not be treated as errors.

## Validation expectations

- Regression coverage exists for:
  - query only
  - single state only
  - multiple states only
  - combined query plus single state
  - combined query plus multiple states
- The ticket-viewer explorer reflects the API result set directly; no client-side workaround is required to restore correctness when query and state filters are combined.

## Related specs

- `ticket-viewer/explorer`

## Code references

- `memory-viewers/memory-api/tools/http/ticket-http/src/serve/handlers/tickets/read.rs`
- `memory-viewers/memory-api/crates/ticket-api/src/storage/store/query.rs`

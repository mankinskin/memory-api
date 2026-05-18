# ticket-http: ticket list endpoint

Canonical contract for the ticket list API consumed by the Dioxus ticket-viewer explorer and related ticket-picking surfaces.

## Endpoint

- `GET /api/tickets`

## Query semantics

- The endpoint accepts an optional free-text query and zero or more state filters through the client contract.
- Unqualified free-text terms search the indexed ticket title and description/body content only; searching by identifier requires `id:<value>`.
- Bare free-text tokens also match partial-word substrings, so `cracker` finds `Firecracker` in title/body content.
- `title:<value>` applies the same substring matching within the title field, so `title:cracker` finds `Firecracker` while still restricting matches to title content.
- Within the `query` string, whitespace-separated terms combine with AND semantics and quoted phrases remain a single free-text term.
- Supported field predicates are `id:<value>`, `title:<value>`, `state:<value>` / `status:<value>`, and `type:<value>` / `ticket_type:<value>`.
- Clients must not advertise unsupported predicate keys as part of the `/api/tickets` query contract.
- Query and state filters are conjunctive: when both are present, a ticket must satisfy the text query and the state filter set.
- When more than one state filter is supplied, the state portion is matched with OR semantics.
- Supplying a query must not bypass or discard active state filters.
- Supplying state filters must not change the text-query matching behavior.
- Filtering is applied before any limit or truncation logic.

## Result contract

- Query-only requests return all tickets matching the indexed free-text terms or supported field predicates.
- State-only requests return all tickets whose state is in the requested state set.
- Combined requests return only tickets that satisfy both conditions.
- Empty result sets are valid and must not be treated as errors.

## Validation expectations

- Regression coverage exists for:
  - query only over title/body content
  - substring partial matches over title/body content
  - exact `id:<uuid>` field predicates
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

## Traceability

- Ticket: `.ticket/tickets/fcced2f3-c32c-4533-9743-56543f428222`
- API validation/code: `memory-viewers/memory-api/tools/http/ticket-http/src/serve/handlers/tickets/tests/listing.rs`
- Contract validation passed:
  - `cargo test -p ticket-http search_list_ -- --nocapture`
  - `cargo build -p ticket-viewer --release && viewer-ctl stop ticket-viewer && viewer-ctl install ticket-viewer && viewer-ctl start ticket-viewer && curl http://127.0.0.1:3002/api/tickets?workspace=default&limit=20&query=cracker`

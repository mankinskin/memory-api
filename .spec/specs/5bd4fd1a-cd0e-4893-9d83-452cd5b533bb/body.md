<!-- aligned-structure:v1 -->

# Summary

Canonical contract for the ticket list API consumed by the Dioxus ticket-viewer explorer and related ticket-picking surfaces.

## Behavior Story

Canonical contract for the ticket list API consumed by the Dioxus ticket-viewer explorer and related ticket-picking surfaces.

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

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

## Workspace-aware contract

- Callers must supply the concrete workspace name published by `GET /api/workspaces`; internal aliases such as `default`, `..`, and `../..` are not part of the public `/api/tickets` contract.
- `active_workspace` and `workspace` in the response must echo the concrete selected workspace name.
- Every returned item must carry a reversible `ticket_ref` that preserves the owning workspace and ticket id for follow-up detail, history, files, and asset requests.
- Parent-opened aggregate queries may return child-owned tickets, but the list response must not rewrite those tickets as locally owned.
- A normal aggregate scan must keep free-text query results aligned with the same child-owned rows surfaced by index-backed list responses; callers must not need a forced reindex before query terms can rediscover those tickets.
- When workspace selection fails, the endpoint must return a typed error envelope with concrete `code`, `message`, and `request_id` fields instead of a generic internal error body.

## Workspace fixture matrix for `/api/tickets`

This endpoint reuses the workspace-topology classes defined in `ticket-api/workspaces/ancestor-dependency-visibility` and `memory-api/workspace`, but only the rows that materially affect list responses belong in this spec's acceptance surface.

| Fixture class | Why `/api/tickets` must cover it | Required `/api/tickets` outcome |
| --- | --- | --- |
| Local baseline | Preserves single-workspace query semantics while workspace naming rules tighten | Local tickets remain local and response workspaces use concrete folder names |
| Parent-opened aggregate workspace | Parent-selected list views can surface child-owned rows from indexed scan roots | `ticket_ref.workspace` preserves child ownership and follow-up requests remain reversible |
| Child-opened ancestor resolution | Child callers must still understand ancestor-owned dependency endpoints that appear in related views | Child-selected list and follow-up flows never rewrite ancestor-owned tickets as child-local |
| Legacy or repaired indexed rows | List responses are index-backed and therefore sensitive to stale persisted path/state metadata | Repaired rows stop surfacing stale ownership, stale state, or unreadable follow-up paths |
| Invalid public workspace identifier | `/api/tickets` is the first public entry point most clients hit | Unknown names and synthetic aliases fail with typed request errors rather than generic 500s |

### Observable validation matrix

| Observable | Local baseline | Parent aggregate | Child or ancestor follow-up | Repaired rows | Invalid workspace input |
| --- | --- | --- | --- | --- | --- |
| `items[].ticket_ref` ownership | Local workspace only | Child-owned rows stay child-scoped | Follow-up requests preserve non-local ownership | Repaired rows stop reporting stale owners | Not applicable |
| `active_workspace` / `workspace` naming | Concrete folder name only | Concrete parent name only | Concrete child or ancestor name only | Repair never reintroduces aliases | Alias requests are rejected |
| Query and filter behavior | Existing text/state semantics unchanged | Aggregated rows still respect query/state filters | Follow-up detail/history/files/assets remain reversible | Stale rows no longer bypass filters with wrong metadata | Error envelope includes `code`, `message`, and `request_id` |

## Validation expectations

- Regression coverage exists for:
  - query only over title/body content
  - substring partial matches over title/body content
  - exact `id:<uuid>` field predicates
  - single state only
  - multiple states only
  - combined query plus single state
  - combined query plus multiple states
- Regression coverage also exists for:
  - concrete workspace-name responses for local and nested aggregate fixtures
  - child-owned `ticket_ref` preservation in parent-selected aggregate list results
  - normal aggregate scans keeping child-workspace tickets searchable by free-text query without `--reindex`
  - typed error envelopes for invalid public workspace identifiers, including synthetic aliases
- The `/api/tickets` response remains the source of truth for combined query and state semantics; explorer-side request ordering guards may prevent stale responses from overwriting newer results, but they must not reinterpret the backend result set.

## Related specs

- `ticket-viewer/explorer`

## Code references

- `memory-api/tools/http/ticket-http/src/serve/handlers/tickets/read.rs`
- `memory-api/crates/ticket-api/src/storage/store/query.rs`

## Traceability

- Ticket: `.ticket/tickets/fcced2f3-c32c-4533-9743-56543f428222`
- Related workspace contract tickets:
  - `C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-api/.ticket/tickets/700b9763-17f8-436e-ace0-45b88bedd1d7`
  - `429f6f1d-6429-4601-bfac-b572fdb4dbff`
  - `C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-api/.ticket/tickets/91011568-ae0b-4b23-b060-b0c018e1e912`
  - `C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/.ticket/tickets/02723a9b-23ff-47b1-8306-0480be087ddd`
- API validation/code: `memory-api/tools/http/ticket-http/src/serve/handlers/tickets/tests/listing.rs`
- Storage validation/code: `memory-api/crates/ticket-api/src/storage/tests.rs`
- Contract validation passed:
  - `cargo test -p ticket-http search_list_ -- --nocapture`
  - `cargo test -p ticket-api scan_keeps_nested_workspace_tickets_searchable_without_reindex -- --nocapture`
  - `cargo run -p ticket-cli -- --json scan`
  - `cargo run -p ticket-cli -- --json search Persist`
  - `curl http://127.0.0.1:3002/api/workspaces`
  - `curl http://127.0.0.1:3002/api/tickets?workspace=<concrete-workspace-name>&limit=20&query=cracker`

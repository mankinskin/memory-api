# Summary

The current best-next interfaces expose only the immediate reverse-degree (`dependees`) signal and do not provide a first-class HTTP next route. That leaves higher-order dependency impact invisible in ranking decisions and forces HTTP consumers to reconstruct best-next results from lower-level endpoints.

## Goals

- Extend best-next ranking beyond immediate reverse-degree so candidate ordering can account for broader dependency-topology impact.
- Keep the ranking contract deterministic and aligned across CLI, MCP, and HTTP surfaces.
- Add a dedicated HTTP route for ranked best-next discovery.

## Required behavior

### Graph-aware ranking

- The best-next contract must define how graph-aware topology signals participate in ranking after workflow progress and priority are applied.
- The design space to evaluate includes transitive reverse-dependency size, PageRank, and betweenness; the implementation may choose one metric or a stable composition of multiple metrics.
- The chosen ranking behavior must remain deterministic for equivalent candidate sets.
- CLI `ticket next` and MCP `next_tickets` must expose the same ranking behavior for equivalent candidate sets.

### HTTP surface

- Ticket HTTP must expose `GET /api/next` as the public endpoint for ranked best-next discovery.
- The HTTP response must not require clients to reconstruct best-next ordering by combining `/api/tickets` with `/api/edges` or `/api/graph/topgraph`.
- The HTTP response must preserve the ranking metadata and board-aware warnings or exclusions needed by consumers to explain results.
- The HTTP ranking order must match the canonical best-next ordering contract used by CLI and MCP.

## Acceptance criteria

- The ranking contract documents how graph-aware topology signals participate in best-next ordering.
- Focused regression coverage proves CLI and MCP produce the same ordering for equivalent candidate sets under the updated contract.
- Focused HTTP coverage proves `GET /api/next` returns ranked candidates directly without requiring multi-endpoint composition.
- HTTP response data is sufficient for clients to render best-next candidates and relevant warnings or exclusions.

## Related specs

- `ticket-api/workflow/best-next-ordering`
- `ticket-http/api/tickets`

## Traceability

- Tracking ticket: C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-viewers/memory-api/.ticket/tickets/d1f9f390-dda0-4762-a14c-9ce339abc393
- Tracking ticket: C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-viewers/memory-api/.ticket/tickets/181ed793-481d-4d46-b059-0eda891365d7

## Validation

- Pending implementation.

# Problem

There is no dedicated `GET /api/next` route in the current ticket HTTP router. HTTP consumers that want ranked best-next results have to reconstruct them manually by combining `GET /api/tickets` with dependency data from `GET /api/edges` or `GET /api/graph/topgraph`.

## Requested improvement

Add a dedicated HTTP next surface that exposes ranked best-next candidates directly from the ticket service.

## Scope

- Add a `GET /api/next` route to the ticket HTTP router.
- Return the same best-next candidate contract and ranking semantics that the CLI and MCP next interfaces expose.
- Surface any board-aware exclusions or warnings needed by HTTP consumers without requiring them to compose multiple endpoints.
- Add focused HTTP regression coverage for the new route.

## Out of scope

- Choosing the final graph-aware ranking heuristic; that is tracked separately.

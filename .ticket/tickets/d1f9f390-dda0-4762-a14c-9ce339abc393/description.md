# Problem

`ticket next` and `ticket-mcp` `next_tickets` currently break ties with immediate reverse-degree (`dependees`) only. That favors tickets with more direct incoming `depends_on` edges, but it does not account for larger transitive reverse-dependency impact or other graph-centrality signals.

## Requested improvement

Define and implement a graph-aware ranking contract for best-next discovery so the ticket interfaces can rank candidates using richer dependency-topology signals than immediate reverse-degree alone.

## Scope

- Evaluate graph-aware signals such as transitive reverse-dependency size, PageRank, and betweenness.
- Choose and document a deterministic ranking contract that stays consistent across CLI and MCP next surfaces.
- Preserve the existing higher-priority ordering keys ahead of any new graph-aware tiebreaker unless the spec explicitly changes that contract.
- Add focused regression coverage for the chosen ordering behavior.

## Out of scope

- Adding the HTTP route itself; that is tracked separately.

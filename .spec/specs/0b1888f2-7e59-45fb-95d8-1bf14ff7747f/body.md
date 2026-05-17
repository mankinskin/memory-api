# Summary

Child ticket workspaces need a way to surface ancestor-owned ticket entries when those parent entries participate directly in dependency relationships with child-owned tickets.

Today, nested workspace work mostly focuses on parent workspaces aggregating descendant data. That leaves a reverse-direction gap: a child workspace can open its own store cleanly, but a child-scoped graph or dependency view cannot fully explain a relationship when the opposite endpoint lives in an ancestor workspace.

## Current seam

- Workspace resolution opens one local ticket index root for the active workspace.
- Ticket and graph views generally assume returned ticket-like records belong to the selected workspace unless a broader workspace-aware contract says otherwise.
- Existing nested workspace planning emphasizes parent aggregation of child results, not child visibility into parent-owned dependency endpoints.

## Required behavior

### Ancestor endpoint visibility

- A child workspace may resolve ticket references from an ancestor workspace when those ancestor tickets are the direct source or target of a dependency relationship involving a child-owned ticket.
- The returned ancestor ticket reference must preserve its owning workspace explicitly.
- Child workspaces must not silently claim or rewrite ancestor-owned tickets as local records.

### Reversible ticket identity

- Any dependency, graph, or related ticket surface that mixes child-owned and ancestor-owned records must return enough identity to map each entry back to one concrete `(workspace, ticket id)` pair.
- Frontend consumers must be able to distinguish local and ancestor-owned dependency endpoints without guessing from route context.

### Compatibility

- Existing single-workspace behavior remains unchanged when no ancestor-child relationship is involved.
- Backward-compatible defaults should remain available for callers that only need local workspace results.

## Traceability

- Ticket: memory-viewers/memory-api/429f6f1d-6429-4601-bfac-b572fdb4dbff
- Adjacent design work: `700b9763-17f8-436e-ace0-45b88bedd1d7` covers parent-selected frontend aggregation of child-workspace tickets; this spec adds the reverse-direction requirement for child visibility into ancestor dependency entries.

## Acceptance criteria

- A child workspace can resolve dependency endpoints owned by an ancestor workspace without dropping the relationship.
- Mixed local and ancestor dependency results preserve explicit workspace ownership per returned ticket reference.
- Dependency and graph consumers can render parent-owned endpoints from a child workspace without inferring ownership from the active route alone.
- The cross-workspace behavior is documented with backward-compatible defaults for local-only callers.

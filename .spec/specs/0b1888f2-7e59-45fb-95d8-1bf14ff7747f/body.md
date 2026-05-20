# Summary

Child ticket workspaces need a way to surface ancestor-owned ticket entries when those parent entries participate directly in dependency relationships with child-owned tickets.

Today, nested workspace work mostly focuses on parent workspaces aggregating descendant data. That leaves a reverse-direction gap: a child workspace can open its own store cleanly, but a child-scoped graph or dependency view cannot fully explain a relationship when the opposite endpoint lives in an ancestor workspace.

## Current seam

- Workspace resolution opens one local ticket index root for the active workspace.
- Ticket and graph views generally assume returned ticket-like records belong to the selected workspace unless a broader workspace-aware contract says otherwise.
- Existing nested workspace planning emphasizes parent aggregation of child results, not child visibility into parent-owned dependency endpoints.
- The current HTTP contract still leaks synthetic workspace aliases such as `default` and `..`, which forces frontend callers to guess when a name is a transport shortcut instead of a concrete owning workspace.
- Storage and resolution failures can still collapse into opaque `500 internal_error` responses even when the backend already knows the missing workspace, ticket, or on-disk path that triggered the failure.

## Required behavior

### Ancestor endpoint visibility

- A child workspace may resolve ticket references from an ancestor workspace when those ancestor tickets are the direct source or target of a dependency relationship involving a child-owned ticket.
- The returned ancestor ticket reference must preserve its owning workspace explicitly.
- Child workspaces must not silently claim or rewrite ancestor-owned tickets as local records.

### Reversible ticket identity

- Any dependency, graph, or related ticket surface that mixes child-owned and ancestor-owned records must return enough identity to map each entry back to one concrete `(workspace, ticket id)` pair.
- Frontend consumers must be able to distinguish local and ancestor-owned dependency endpoints without guessing from route context.

### Concrete workspace names

- Every public workspace identifier emitted by ticket HTTP endpoints must use the owning workspace folder name.
- Synthetic aliases such as `default`, `..`, and `../..` are not valid public workspace names for list, detail, history, graph, edge, or workspaces responses.
- The primary workspace exposed by a single opened store must use that store's workspace folder name as `active_workspace`, and follow-up requests must reuse that exact name.

### Actionable error envelopes

- Workspace and ticket resolution failures must return typed error envelopes with actionable `code` and `message` fields instead of a generic `internal_error` body.
- Missing on-disk ticket data, invalid workspace names, and similar resolution misses must not collapse into an opaque 500 when the backend can classify them as a concrete failure mode.
- If a request still reaches a true internal failure path, the response must identify the failed operation and preserve the `request_id` so the caller can report the issue without guessing.

### Compatibility

- Existing single-workspace behavior remains unchanged when no ancestor-child relationship is involved.
- Local-only callers still receive a single active workspace, but that workspace is identified by its concrete folder name rather than a synthetic alias.

## Traceability

- Ticket: memory-viewers/memory-api/429f6f1d-6429-4601-bfac-b572fdb4dbff
- Adjacent design work: `700b9763-17f8-436e-ace0-45b88bedd1d7` covers parent-selected frontend aggregation of child-workspace tickets; this spec adds the reverse-direction requirement for child visibility into ancestor dependency entries.

## Acceptance criteria

- A child workspace can resolve dependency endpoints owned by an ancestor workspace without dropping the relationship.
- Mixed local and ancestor dependency results preserve explicit workspace ownership per returned ticket reference.
- Dependency and graph consumers can render parent-owned endpoints from a child workspace without inferring ownership from the active route alone.
- Root and nested workspace endpoints expose folder-name workspace identifiers only; no public response emits `default` or relative-path aliases.
- Resolution failures return actionable error envelopes that identify the concrete storage or workspace failure mode instead of a generic `internal_error` message.

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

### Authoritative ticket folder traceability

- Query-oriented ticket tooling used for review, specs, and handoffs must be able to report the authoritative on-disk ticket folder path for a returned ticket reference.
- The reported folder path must come from indexed or resolved ticket metadata, not from reconstructing `.ticket/tickets/<id>` in the caller.
- When a caller needs to turn a returned ticket reference into a traceability link, the path-producing command must agree with the workspace ownership already reported for that ticket.

### Concrete workspace names

- Every public workspace identifier emitted by ticket HTTP endpoints must use the owning workspace folder name or a collision-safe derivative of that name that remains reversible to one owning workspace.
- `GET /api/workspaces` must publish the authoritative machine-readable identifier as `name` and the human-facing workspace label as `label`.
- When `name` and `label` differ, transport and follow-up requests must continue to use `name`; `label` is display-only and must not be treated as a reversible identifier.
- Viewer routes and request construction must key off `name` while rendering `label` in user-facing workspace chrome.
- Synthetic aliases such as `default`, `..`, and `../..` are not valid public workspace names for list, detail, history, graph, edge, or workspaces responses.
- The primary workspace exposed by a single opened store must use that store's workspace folder name as `active_workspace`, and follow-up requests must reuse that exact name.

### Actionable error envelopes

- Workspace and ticket resolution failures must return typed error envelopes with actionable `code` and `message` fields instead of a generic `internal_error` body.
- Missing on-disk ticket data, invalid workspace names, and similar resolution misses must not collapse into an opaque 500 when the backend can classify them as a concrete failure mode.
- If a request still reaches a true internal failure path, the response must identify the failed operation and preserve the `request_id` so the caller can report the issue without guessing.

### Compatibility

- Existing single-workspace behavior remains unchanged when no ancestor-child relationship is involved.
- Local-only callers still receive a single active workspace, but that workspace is identified by its concrete folder name rather than a synthetic alias.

## Workspace Fixture Strategy

This contract needs more than one happy-path fixture. Workspace behavior changes depending on which store is opened, whether the active workspace is parent or child scoped, and whether the index already contains legacy or stale rows from an earlier scan. A complete test plan must therefore treat workspace topology and persisted index state as first-class inputs rather than incidental setup details.

The same logical relationship should be exercised through both repository-root entry points and direct hidden-store entry points whenever that distinction is meaningful. That guards against the class of bugs where a workspace behaves correctly when opened through a repo root but diverges when opened through the `.ticket` store itself, or vice versa.

### Workspace topology fixture matrix

| Fixture class | Opened workspace entry point | Topology / stored state | Why it exists | Required outcome |
| --- | --- | --- | --- | --- |
| Local baseline | Repo root and direct `.ticket` root for one store | No ancestor-child cross-workspace edge | Proves nested-workspace support does not regress single-workspace behavior | Local tickets remain local, concrete workspace names stay stable, and no ancestor ownership is invented |
| Parent-opened aggregate workspace | Parent repo root and parent `.ticket` root | Parent store aggregates one or more child scan roots | Validates parent-selected views that include child-owned tickets | Parent queries may surface child-owned records, but ownership remains child-scoped and reversible |
| Child-opened ancestor resolution | Child repo root and child `.ticket` root | Child-owned ticket is directly linked to an ancestor-owned endpoint | Covers the reverse-direction behavior defined by this spec | Child queries surface ancestor endpoints without dropping the relationship or rewriting ownership as local |
| Legacy relative indexed path | Any workspace that already indexed the ticket | Existing index row stores a recoverable relative ticket path | Covers backward-compatible path recovery from older persisted rows | Follow-up detail reads normalize to the concrete ticket folder and remain readable across list/get/history-style flows |
| Corrupted absolute indexed path plus stale metadata | Parent aggregate workspace | Existing row points at the wrong absolute/store-local path and may carry stale title, state, type, or `created_at` | Covers self-healing of parent aggregate indexes after child-workspace changes | Normal and forced scans repair the indexed row back to the on-disk child ticket and clear stale metadata |
| Minimal manifest fixture | Any workspace | On-disk manifest omits optional `type`, `title`, or `state` fields | Documents deterministic behavior for manually authored or partially migrated manifests | Scans must index predictable defaults instead of panicking or producing topology-dependent behavior |
| Deleted-on-disk fixture | Any workspace that previously indexed the ticket | Ticket folder is physically deleted on disk | Ensures stale aggregate rows do not outlive the source-of-truth manifest state | Reindexing prunes the row so downstream dependency endpoints do not surface deleted tickets |
| Invalid public workspace identifier | HTTP or transport-facing caller | Request uses an unknown workspace name or a synthetic alias such as `default` / `..` | Validates the public workspace contract, not just happy-path storage behavior | Response returns an actionable typed error envelope instead of silently accepting aliases or collapsing into a generic internal error |

### Observable requirement matrix

The fixture classes above are only useful if every relevant surface is checked against them. The implementation should therefore reuse the same fixture graph across storage, HTTP, and downstream consumer tests whenever possible.

| Observable surface | Local baseline | Parent-opened aggregate | Child-opened ancestor | Legacy / corrupted row repair | Minimal / deleted folder | Invalid workspace input |
| --- | --- | --- | --- | --- | --- | --- |
| `list` / index-backed summaries | Preserve local-only behavior | Include child-owned rows with explicit ownership | Preserve ancestor endpoint ownership when surfaced from child context | Repaired rows stop reporting stale state or wrong ownership | Deterministic defaults or pruning; no ghost rows | Not applicable |
| Detail follow-ups (`get`, history, files, assets) | Resolve local ticket folders | Resolve child-owned folders from parent-selected results | Resolve ancestor-owned folders from child-selected results | Normalize repaired paths before reading manifests or descriptions | Missing optional fields remain readable; deleted tickets no longer resolve as active | Return typed request errors instead of opaque 500s |
| Dependency / graph traversal | Local edges stay unchanged | Mixed parent/child edges preserve endpoint ownership | Child views keep ancestor endpoints instead of dropping them | Traversal reflects repaired ticket identity after scan | Deleted endpoints do not leak back into the graph | Invalid workspace names are rejected before traversal |
| Workspace naming in responses | Canonical `name` and display `label` stay stable for the same workspace | Parent and child responses preserve reversible `name` values and readable labels | Child and ancestor responses keep explicit `name` ownership while exposing human-readable labels | Repair never reintroduces synthetic aliases or stale labels | Defaults and pruning do not affect naming rules | Synthetic aliases are treated as invalid public input |
| Scan / reconciliation behavior | No-op behavior remains stable | Aggregate scan keeps parent-child ownership intact | Child workspace scan keeps ancestor relationships resolvable | Both normal and forced scan repair stale persisted rows | Minimal manifests default consistently; deleted folders are pruned | Not applicable |
| Error envelope behavior | Classified failures remain actionable | Aggregate resolution misses stay typed | Ancestor lookup misses stay typed | Wrong-path failures are repaired or reported concretely, never hidden behind generic internal errors | Incomplete manifests fail predictably; deleted folders are absent | Must return concrete `code` and `message` fields |

## Traceability

- Related ticket id: `429f6f1d-6429-4601-bfac-b572fdb4dbff`.
	A live `ticket get` path lookup for this older ticket still fails with a storage I/O path error in the current store, so this spec keeps the id rather than synthesizing a folder path.
- Adjacent design work: `C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-api/.ticket/tickets/700b9763-17f8-436e-ace0-45b88bedd1d7` covers parent-selected frontend aggregation of child-workspace tickets; this spec adds the reverse-direction requirement for child visibility into ancestor dependency entries.
- Tooling traceability work: `C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-api/.ticket/tickets/91011568-ae0b-4b23-b060-b0c018e1e912` adds authoritative ticket-folder-path output so specs and reviews can record exact ticket folders without reconstructing them.

## Acceptance criteria

- A child workspace can resolve dependency endpoints owned by an ancestor workspace without dropping the relationship.
- Mixed local and ancestor dependency results preserve explicit workspace ownership per returned ticket reference.
- Dependency and graph consumers can render parent-owned endpoints from a child workspace without inferring ownership from the active route alone.
- Query-oriented ticket tooling can report the authoritative ticket folder path for a returned mixed-workspace ticket without reconstructing it client-side.
- Root and nested workspace endpoints expose folder-name workspace identifiers only; no public response emits `default` or relative-path aliases.
- `/api/workspaces` distinguishes the canonical reversible workspace identifier from the user-facing display label, and viewer routes render the label without losing the canonical id.
- Resolution failures return actionable error envelopes that identify the concrete storage or workspace failure mode instead of a generic `internal_error` message.
- The spec defines a reusable workspace-topology fixture matrix that covers local baseline, parent-opened aggregation, child-opened ancestor resolution, relative-path recovery, corrupted absolute-path repair, minimal manifests, physically deleted folders, and invalid public workspace identifiers.
- The verification matrix explicitly maps list/detail/history/files/assets/dependency/graph/naming/error-envelope assertions to those fixture classes.
- Where repo-root and direct `.ticket` entry points are both valid for the same logical workspace, the validation plan exercises both and requires equivalent ownership and resolution outcomes.

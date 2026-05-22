# Summary

The current workflow surface has two strong but separate pieces:

- default `ticket next` can rank actionable work using shared dependency-convergence pressure
- `ticket unblocked-by <id>` and `ticket next <id>` can scope analysis around one prerequisite root

What is still missing is a tree-oriented workflow surface that lets operators inspect dependency structure in both directions and then drive work back toward natural execution order across CLI, HTTP, and viewer surfaces.

This blueprint adds two complementary capabilities:

- `ticket blockers <id>` for upstream blocker-tree exploration beneath a focus ticket or epic
- a tree-oriented `ticket unblocked-by <id>` that preserves direct parent-child structure below the queried prerequisite root and weights impacted parents by how close they are to being fully unblocked

It also extends ordering with a first-class recent-unblock signal so actionable work that has just become available can rise without defeating stronger convergence pressure, and it defines how that ordering is preserved when exposed through `ticket-http` and rendered in `ticket-viewer`.

## Goals

- Add an upstream blocker tree for a selected ticket or epic.
- Upgrade downstream unlock discovery from flat lists to ordered nested trees.
- Preserve one shared workflow model in `ticket-api` for CLI, MCP, `ticket-http`, `ticket-viewer`, board, health, and audit consumers.
- Define exact semantics for recently unblocked work and separate them from generic blocker progress.
- Keep large-store behavior efficient through indexed workflow facts and targeted graph traversal.

## Definitions

- `became_actionable_at`: the timestamp when a ticket's unresolved `depends_on` count transitions from greater than zero to zero, making it actionable under the normal workflow rules.
- `last_blocker_progress_at`: the timestamp when any unresolved blocker on the upstream path changed in a way that moved the subtree forward, even if the ticket is still blocked.
- `blocker tree`: the recursive upstream `depends_on` tree rooted at a queried ticket, where direct children are direct blockers.
- `unlock tree`: the recursive reverse-dependency tree rooted at a queried ticket, where direct children are direct dependents.
- `frontier leaf`: a leaf node in the relevant tree direction that is also the next direct work item for advancing the subtree.
- `unresolved_frontier_leaf_count`: the number of frontier leaves that still need action before a node becomes fully actionable or fully unblocked.
- `blocker distance`: a depth or count-based measure of how many unresolved frontier leaves remain beneath a node.
- `workflow-authored order`: the canonical server-side ordering produced from shared workflow facts; transport and UI layers must preserve it for workflow-specific surfaces instead of re-sorting locally with generic fields.

## Command model

### `ticket blockers <id>`

- Resolve the queried ticket using the same id or prefix rules as the rest of the CLI.
- Walk upstream `depends_on` edges recursively.
- Return a nested blocker tree rooted at the queried ticket.
- Preserve direct child structure: each node's `children[]` represent direct blockers, not a flattened transitive set.
- Return all deep blocked tickets in the tree and emphasize frontier leaves because those are the likely first tickets to work on.
- Include lagging-state evidence so the command can focus an operator on blockers that are behind their dependents in workflow progression.
- Order each sibling set by:
  1. lower `unresolved_frontier_leaf_count`
  2. smaller blocker distance to a frontier leaf
  3. larger dependency-state gap pressure against downstream dependents
  4. explicit priority
  5. deterministic fallback ordering

### `ticket unblocked-by <id>`

- Continue to treat the queried root as satisfied for analysis.
- Walk reverse `depends_on` edges recursively.
- Return a nested unlock tree rooted at the queried prerequisite.
- Preserve direct child structure: each node's `children[]` represent direct dependents below the root.
- Include both fully actionable dependents and still-blocked impacted dependents in the tree instead of reducing the result to flat slices.
- Weight each unblocked or nearly-unblocked parent by how close it is to being completely unblocked, primarily via `unresolved_frontier_leaf_count` and blocker distance.
- Surface the deepest blocked subtree leaves and frontier leaves explicitly so operators can jump to the leaf work that will move the parent node forward fastest.
- Support a derived frontier view in addition to the tree so callers can ask both:
  - what is the full dependency shape below this root?
  - which leaf tickets should be worked first to advance it?

### `ticket next`

- Continue to return only globally actionable tickets.
- Reuse the shared convergence pressure ordering already implemented in `ticket-api`.
- Add a recent-unblock ordering component for tickets that have just become actionable.
- Keep `became_actionable_at` distinct from `last_blocker_progress_at`; only the former should affect the global actionable queue.

### `ticket-http`

- Add HTTP parity for the workflow surfaces instead of limiting the feature to CLI and MCP consumers.
- Expose ordered workflow payloads for:
  - actionable next or recommendation views
  - upstream blocker trees
  - downstream unlock trees
  - frontier or leaf-focused follow-up views when the caller only needs the direct next work items
- Treat response array order as authoritative `workflow-authored order`; handlers should not degrade it into generic title, state, or updated-time ordering.
- Return the workflow metadata needed by UI clients to explain rank and subtree progress, including at least:
  - `unresolved_frontier_leaf_count`
  - `frontier_leaf_ids[]`
  - `blocker_distance`
  - `dependency_state_gap`
  - `affected_reverse_dependent_reach`
  - `became_actionable_at`
  - `last_blocker_progress_at`

### `ticket-viewer`

- Keep the generic list view's user-selected field sorts for plain browsing, but add workflow-focused surfaces that use shared workflow ordering rather than local field-based sorting.
- Add viewer flows for:
  - actionable next or recommendation lists ordered by the shared comparator
  - upstream blocker trees
  - downstream unlock trees
- Preserve direct parent-child hierarchy and frontier-leaf emphasis in the UI instead of flattening workflow trees into generic tables.
- Preserve `workflow-authored order` from `ticket-http`; the viewer should not locally re-sort workflow payloads by title, state, created-at, or updated-at once they have been delivered in canonical workflow order.
- Surface enough rank evidence in the UI for operators to understand why a parent node or actionable item rose, including recent-unblock timing and frontier-leaf counts where relevant.

## Ordering contract

### Global actionable ordering

The shared comparator should become:

1. convergence pressure
   - higher `max_affected_dependent_state`
   - larger `dependency_state_gap`
   - larger `affected_reverse_dependent_reach`
2. more recent `became_actionable_at`
3. explicit priority
4. candidate workflow progress
5. transitive reverse-dependent count
6. immediate dependees
7. created_at
8. deterministic fallback

This keeps the system dependency-first while still favoring work that has only just become available.

### Tree ordering

For both `blockers` and `unblocked-by`, sibling order should not reuse the global next comparator verbatim. Tree ordering should favor subtree progress:

1. lower `unresolved_frontier_leaf_count`
2. smaller blocker distance to frontier leaves
3. stronger convergence pressure or state-gap evidence
4. more recent `last_blocker_progress_at`
5. explicit priority
6. deterministic fallback

That makes near-unblock parent nodes rise ahead of broad but still distant subtrees.

### Transport and viewer ordering

- `ticket-http` must emit workflow-specific arrays in canonical comparator order.
- `ticket-viewer` must preserve that order for workflow-oriented panels or routes.
- Generic ad hoc viewer sort keys remain available for plain ticket browsing, but they are not the source of truth for workflow recommendations or tree payloads.

## Shared data model

`ticket-api` should own a reusable tree and workflow-facts surface, for example:

- `WorkflowFacts`
  - `unresolved_dependency_count`
  - `became_actionable_at`
  - `last_blocker_progress_at`
  - `state_index`
  - convergence metrics already used by `next`
- `WorkflowTreeNode`
  - `ticket_id`, `title`, `state`, `priority`
  - `children[]`
  - `remaining_blocker_count`
  - `unresolved_frontier_leaf_count`
  - `frontier_leaf_ids[]`
  - `blocker_distance`
  - `dependency_state_gap`
  - `affected_reverse_dependent_reach`
  - `transitive_reverse_dependents`
  - `became_actionable_at`
  - `last_blocker_progress_at`

The workflow module should expose one canonical builder for:

- upstream blocker trees
- downstream unlock trees
- derived frontier leaves
- per-node ranking keys
- transport-ready workflow payloads that can be reused by `ticket-http`, MCP, and the viewer without recomputing sort order in each surface

## Index and efficiency strategy

The current store already separates graph metadata from full-text search. This blueprint should preserve that split.

- Graph traversal should use the edge index and materialized workflow facts, not Tantivy.
- Tantivy search should be used only for text-heavy root discovery, title filtering, or large search prefilter steps.
- Tree commands should traverse only the reachable subgraph around the queried root, not rebuild full-store graph state for every request.
- Global actionable ordering should read precomputed workflow facts instead of scanning ticket history on each request.
- `ticket-http` handlers and `ticket-viewer` data fetches should consume shared workflow results directly; they must not rebuild workflow ranking client-side from raw ticket lists.
- `became_actionable_at` and `last_blocker_progress_at` should be materialized into the index and updated incrementally on:
  - ticket state changes
  - `depends_on` edge mutations
  - ticket closure or cancellation
- Updates should propagate only across affected reverse dependents via targeted BFS or queue-based recomputation.
- Append-only history remains the durable source of truth, but per-query history scans should be avoided for large stores.

## Surface rollout

- Phase 1: shared workflow facts and tree builders in `ticket-api`
- Phase 2: CLI `blockers` plus nested-tree `unblocked-by`
- Phase 3: global next and board ordering integrate `became_actionable_at`
- Phase 4: `ticket-http` exposes workflow trees, frontier views, and actionable ordering metadata
- Phase 5: MCP parity for tree payloads and ranking metadata
- Phase 6: `ticket-viewer` adopts workflow-specific ordering and tree views without client-side re-sorting
- Phase 7: focused performance validation on large ticket graphs

## Acceptance criteria

- `ticket blockers <id>` returns a nested upstream tree with all deep blockers and emphasizes frontier leaves.
- `ticket unblocked-by <id>` returns a nested downstream tree that preserves direct parent-child structure and exposes frontier leaves for quick follow-up work.
- Parent nodes in `unblocked-by` are ordered by closeness to being fully unblocked, measured primarily by unresolved frontier leaves and blocker distance.
- The shared comparator for global actionable work uses `became_actionable_at` after convergence pressure.
- `ticket-http` exposes ordered workflow payloads and rank metadata for actionable next, blockers, and unlock-tree consumers.
- `ticket-viewer` preserves server-authored workflow order for workflow-specific surfaces and renders blocker/unlock trees with direct hierarchy and frontier emphasis.
- The implementation plan avoids full history scans and uses indexed workflow facts for large graph queries.
- The ticket set covers workflow facts, indexed propagation, CLI tree rendering, `ticket-http` parity, `ticket-viewer` integration, ranking integration, and MCP parity.

## Related specs

- `ticket-api/workflow/unblocked-by-discovery`
- `ticket-api/workflow/best-next-ordering`
- `ticket-api/workflow/dependency-convergence-ranking`

## Validation plan

- `cargo test -p ticket-api workflow:: -- --nocapture`
- `cargo test -p ticket-cli blockers --test integration_board_cli -- --nocapture`
- `cargo test -p ticket-cli unblocked_by --test integration_board_cli -- --nocapture`
- `cargo test -p ticket-http -- --nocapture`
- `cargo test -p ticket-mcp next_tickets_ -- --nocapture`
- `npm --prefix memory-viewers/ticket-viewer/frontend/dioxus run test:e2e:release -- ticket-viewer.release.spec.ts`
- focused storage or benchmark coverage for incremental workflow-fact propagation

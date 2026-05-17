# Problem

Child memory workspaces such as `ticket-api` can currently resolve their own ticket entries, but cross-workspace dependency views break down when one side of the edge lives in an ancestor workspace.

That means a child-scoped viewer or API call can show the local ticket while dropping, hiding, or failing to resolve dependency targets that belong to the parent workspace. The result is incomplete dependency graphs and misleading ticket context.

# Goal

Define and implement the ticket workspace behavior needed for a child workspace to remain aware of parent workspace entries when rendering or returning dependencies.

This work should make cross-workspace dependency relationships reversible and explicit, without making ordinary single-workspace flows more ambiguous.

# Scope

- child workspace reads that need to resolve dependency targets or sources from an ancestor workspace
- workspace-aware ticket references for dependency and graph surfaces
- provenance rules so clients know which workspace owns each returned ticket-like record
- compatibility rules for existing single-workspace callers

# Constraints

- Existing single-workspace behavior must remain valid when no parent-child workspace relationship exists.
- Parent ownership must stay explicit; child workspaces must not silently re-home parent tickets.
- Any API or storage contract changes should preserve enough identity to map a dependency endpoint back to one concrete `(workspace, ticket id)` pair.

# Acceptance Criteria

- A child workspace can resolve dependency endpoints that belong to an ancestor workspace without dropping the relationship.
- Returned dependency or graph data preserves workspace identity for both local and ancestor-owned ticket references.
- Ticket-viewer and other frontend consumers can distinguish parent-owned dependency entries from child-owned entries without guessing.
- The cross-workspace behavior is documented with backward-compatible defaults for callers that still operate on one workspace at a time.

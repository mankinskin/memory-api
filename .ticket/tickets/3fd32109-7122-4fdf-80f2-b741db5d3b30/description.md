# Goal

Provide infrastructure for a ticket-viewer graph mode that can keep the whole workspace graph visible while focusing the selected ticket.

# Scope

- expose or derive a workspace-scoped graph payload suitable for full-graph rendering
- avoid per-selection subgraph refetches for focus changes when the full graph is already available
- preserve enough relationship metadata to highlight direct and near-neighbor dependencies efficiently
- keep the payload practical for repeated UI updates and cache reuse

# Acceptance

- the frontend can request a graph payload that represents the workspace graph rather than a fixed-depth subgraph rooted at one ticket
- the payload supports selecting a focus ticket without needing to refetch a new root-local graph on every selection
- performance characteristics are documented well enough to guide caching and culling work in the viewer

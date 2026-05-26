# Goal

Change the ticket-viewer graph mode so the full graph stays visible while the selected ticket becomes the active focus anchor.

# Scope

- render the full graph dataset in graph mode
- pan or center the viewport toward the focused ticket instead of replacing the graph with a different root-local fetch
- highlight the focused ticket and related edges or nearby nodes
- fade or cull distant unrelated nodes enough to keep the focused neighborhood readable

# Acceptance

- switching ticket focus updates highlighting and camera focus without replacing the whole graph dataset
- the overall workspace graph remains visible in graph mode
- related edges and nodes are emphasized while distant unrelated graph regions are visually de-emphasized
- the behavior works smoothly when switching among nearby tickets in the same workspace

# Problem

The current default graph layouts are tuned for planar hierarchy views, but they do not present ticket dependency trees in a way that matches ticket workflow review. Dense ticket sets with mixed states are hard to scan by workflow stage and by related dependency clusters.

# Goals

- add a kanban-style graph layout mode with workflow states as columns
- keep more horizontal space between workflow columns so the lanes read clearly at a glance
- group dependency-related subcomponents into stable row cells within each state column
- keep multiple nodes legible inside a single cell with soft clustering instead of exact overlap
- render visible state-column headers, separators, and row labels so the kanban structure stays readable at a glance
- make the kanban guide labels grow enough to stay readable when the graph is zoomed in and shrink to more compact sizes and line heights when the graph is zoomed far out
- keep the left-side row labels clear of enlarged nearby node cards instead of letting the guide overlay overlap visible nodes

# Validation

- focused viewer-api or ticket-viewer tests covering the kanban layout grouping logic
- release Playwright coverage proving the layout mode can be selected, renders visible headers and row labels, preserves state-column placement for a real ticket graph, keeps row labels readable at close zoom, avoids row-label overlap with visible nodes, and shrinks row-label height when zoomed out

# Implementation

Add a third graph layout mode to the shared Graph3D settings flow, implement the table or kanban placement algorithm in ticket-viewer layout generation, widen the workflow-lane spacing, add visible overlay affordances for column headers, separators, and row labels, size those guide labels from the same camera-aware model that drives node readability, keep the row labels clear of enlarged visible node cards, and extend graph browser validation to assert both the lane affordances and the column-by-state placement.
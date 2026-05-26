# Objective

Upgrade the ticket-viewer main layout so ticket details render as a compact integrated document and the graph view becomes a focused full-workspace navigation surface with better layout, settings, and level-of-detail rendering.

# Scope

- unify title, metadata, description, and asset files into one contextual ticket document area
- keep the full ticket graph visible while panning and highlighting the focused ticket and its related edges
- correct layout defaults so dependency hierarchy reads cleanly in an isometric-oriented 2D plane
- introduce multiple node detail levels so zoomed-out graphs stay legible

# Child tickets

- integrated ticket document panel
- workspace graph payload for focused full-graph navigation
- focused full-graph viewer behavior
- hierarchy-preserving layout defaults and graph settings
- node level-of-detail rendering

# Non-goals for this tracker

- replacing the entire viewer shell or panel system
- rewriting unrelated explorer, search, or history behavior unless needed for the new document surface

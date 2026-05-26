# Goal

Introduce multiple graph node detail levels so zoomed-out views stay legible and zoomed-in views can show rich ticket content.

# Scope

- define several node detail tiers, from minimal dot or particle forms up through compact and full ticket-card rendering
- choose detail levels from zoom, focus state, or visibility budget
- trim text length and auxiliary metadata aggressively at lower detail levels
- allow focused nodes to stay richer than distant nodes when the graph is dense

# Acceptance

- the graph can render nodes at several distinct detail levels instead of a single fixed card presentation
- zoomed-out graphs avoid unreadable text walls by falling back to minimal representations
- zooming in restores progressively richer ticket content
- focus or hover state can keep the active node more informative than distant nodes at the same zoom level

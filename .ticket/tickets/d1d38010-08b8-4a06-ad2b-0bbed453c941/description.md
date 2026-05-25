# Problem

The shared Graph3D surface preserves dragged node positions only until the next same-graph re-render. Once the ticket-viewer reuses the same workspace graph payload or changes node focus, the shared sync path replaces the user's local node positions with the backend layout and autofocus also resets the camera zoom distance.

# Goals

- preserve dragged node positions while the mounted graph keeps rendering the same topology
- keep focus changes inside the mounted graph from snapping the layout back to backend coordinates
- retarget the camera on node focus changes without replacing the user's zoom level

# Validation

- focused shared graph regression coverage for same-topology layout reconciliation
- focused ticket-viewer Playwright coverage for drag persistence and focus-preserved zoom

# Implementation

- preserve same-topology incoming graph payloads by copying the current dragged node coordinates into the refreshed shared Graph3D layouts when the layout mode stays the same
- keep focus changes inside the mounted graph from replacing the current zoom distance by retargeting the camera with the existing distance instead of reframing
- expose the live camera distance on the graph container so the browser regression can assert zoom preservation directly
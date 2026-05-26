# Goal

Fix graph layout defaults and settings so dependency hierarchy reads cleanly from top to bottom on a 2D plane optimized for isometric viewing.

# Scope

- correct parent-child ordering so the layout follows dependency hierarchy more reliably
- reduce overlap from the current 3D spread and force behavior
- choose a default layout and camera preset that reads as an isometric diagram rather than a cluttered depth stack
- expose the important layout parameters in graph settings instead of hard-coding them

# Acceptance

- parent and child layers read consistently from top to bottom in the dependency hierarchy
- the default layout avoids common node overlap cases seen in the current graph mode
- the default camera and projection make the graph readable as a mostly planar isometric view
- graph settings expose the main layout controls needed to tune spacing and hierarchy behavior

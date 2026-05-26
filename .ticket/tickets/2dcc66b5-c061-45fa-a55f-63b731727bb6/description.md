# Goal

Replace the split metadata/content treatment with a single compact ticket document area in the main layout.

# Scope

- show the ticket title above the main description/body area
- integrate important metadata into the same document surface instead of hiding it in a separate side panel
- present asset files as part of one contextual document flow instead of a bare disconnected file viewer whenever possible
- keep edit and history affordances reachable without fragmenting the primary reading surface

# Acceptance

- the selected ticket renders as one primary document surface in the main layout
- title, key metadata, and description are visible in the same reading context without opening a separate metadata panel
- asset content is contextualized within the document area rather than shown only as a raw detached tab
- the new document remains compact enough to work in split mode next to the graph

# Test design

- open a ticket in split mode and verify the title, metadata summary, and description appear in one scrollable document area
- open a ticket with at least one asset file and verify the document view exposes that asset context without losing the primary ticket body
- resize the main layout and verify the document remains readable in both split and content-only modes

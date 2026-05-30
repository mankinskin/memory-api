# Nested workspace resolution

A repository may contain multiple `memory-api` workspaces (for example the context-engine root, `memory-viewers/memory-api`, and `memory-viewers/viewer-api`, each with their own `.ticket/`, `.spec/`, `.rule/`). The workspace resolver normalises any caller-supplied path to a single owning root and never falls back silently to an ancestor checkout.
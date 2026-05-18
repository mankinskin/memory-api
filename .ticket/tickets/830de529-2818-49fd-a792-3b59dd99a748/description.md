Fix spec creation target-root handling so spec entities are always created inside the canonical .spec/specs store when the caller passes a workspace root, the .spec store root, or a path inside that store.

Acceptance criteria:
- `spec.exe create --index-root .spec --root .` creates the new spec under `.spec/specs/<id>/` instead of writing a folder at repo root.
- `SpecStore::create` rejects target roots outside the registered scan roots and the local `.spec` store.
- Focused regression tests cover valid workspace/store-root inputs and an invalid external root.
- The spec CLI help text describes the normalized target-root behavior instead of implying literal placement.

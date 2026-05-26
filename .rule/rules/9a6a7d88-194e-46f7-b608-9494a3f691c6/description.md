## Resolution rules

1. Find the nearest registered scan root that contains the path. If none exists, fall back to the nearest directory upward that contains a `.ticket/`, `.spec/`, or `.rule/` store.
2. If the path is inside a nested workspace, the nested workspace wins. Ancestor stores are not consulted for entities the nested workspace owns.
3. Ambiguous paths (matching more than one nested workspace, or matching no workspace at all) fail with `code: invalid_request` rather than picking arbitrarily.
<!-- spec-api:file generated=true -->

<!-- spec-api:entry id=30e77f04-1d48-4e27-82dd-864b9a92d9c4 slug=memory-api/recurring-principles/workspace-identifiers/concrete-workspace-identifiers/l1 -->
# Concrete workspace identifiers

`memory-api` stores (`ticket-api`, `spec-api`, `rule-api`, `doc-api`, `audit-api`) accept only concrete workspace paths. Synthetic aliases such as `default`, `..`, `~`, empty strings, or arbitrary handles are rejected with a typed error.

<!-- spec-api:entry id=5253e1df-d8d3-4174-8037-6e088ac58e7c slug=memory-api/recurring-principles/workspace-identifiers/concrete-workspace-identifiers/resolver-contract/l5 -->
## Resolver contract

- `--workspace-root <PATH>` must point at an existing directory that contains the relevant store (or is an ancestor that can be normalised to exactly one such store).
- The resolver normalises the path to the canonical root that owns the store. Ambiguous paths that match more than one nested workspace fail rather than picking one silently.
- `--index-root` follows the same rules and is independent of `--workspace-root`: callers may target an alternate index without changing the workspace.

<!-- spec-api:entry id=b35856fb-f3b5-4673-b7ba-b5af240b4fb2 slug=memory-api/recurring-principles/workspace-identifiers/concrete-workspace-identifiers/rejected-inputs/l11 -->
## Rejected inputs

- The literal string `default` (legacy alias).
- `.` or `..` resolved at the call site when those expand outside any registered scan root.
- Paths that do not resolve to a checkout containing a `.ticket/`, `.spec/`, or `.rule/` directory after walking up to the nearest registered root.

<!-- spec-api:entry id=2d133b3a-d255-4543-8132-c5bbfb14bf0d slug=memory-api/recurring-principles/workspace-identifiers/concrete-workspace-identifiers/rejected-inputs/l17 -->
The rejection produces a `code: invalid_request` error envelope with a `message` that names the rejected input. CLIs print the envelope on stderr (or to stdout under `--json`) and exit non-zero.

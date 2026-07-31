# Concrete workspace identifiers

`memory-api` stores (`ticket-api`, `spec-api`, `rule-api`, `doc-api`, `audit-api`) accept only concrete workspace paths. Synthetic aliases such as `default`, `..`, `~`, empty strings, or arbitrary handles are rejected with a typed error.

## Resolver contract

- `--workspace-root <PATH>` must point at an existing directory that contains the relevant store (or is an ancestor that can be normalised to exactly one such store).
- The resolver normalises the path to the canonical root that owns the store. Ambiguous paths that match more than one nested workspace fail rather than picking one silently.
- `--index-root` follows the same rules and is independent of `--workspace-root`: callers may target an alternate index without changing the workspace.
- For **cross-workspace moves**, both `--source-workspace` and `--target-workspace` must resolve to valid, concrete, distinct workspaces on disk. If either path is empty, ambiguous, or fails to resolve to a unique store, the move is rejected with `code: invalid_request`.

## Rejected inputs

- The literal string `default` (legacy alias).
- `.` or `..` resolved at the call site when those expand outside any registered scan root.
- Paths that do not resolve to a checkout containing a `.ticket/`, `.spec/`, or `.rule/` directory after walking up to the nearest registered root.
- In move operations, any source or target identifier resolving to the exact same workspace root is rejected. Moves must be between distinct concrete stores.

The rejection produces a `code: invalid_request` error envelope with a `message` that names the rejected input. CLIs print the envelope on stderr (or to stdout under `--json`) and exit non-zero.

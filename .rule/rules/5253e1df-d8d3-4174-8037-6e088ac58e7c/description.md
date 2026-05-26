## Resolver contract

- `--workspace-root <PATH>` must point at an existing directory that contains the relevant store (or is an ancestor that can be normalised to exactly one such store).
- The resolver normalises the path to the canonical root that owns the store. Ambiguous paths that match more than one nested workspace fail rather than picking one silently.
- `--index-root` follows the same rules and is independent of `--workspace-root`: callers may target an alternate index without changing the workspace.
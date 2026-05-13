# workspace

Source: `crates/memory-api/src/workspace.rs`

## Public API

### `WorkspaceConfig` (Struct)

### `WorkspaceConfig` (Impl)

### `find_local_workspace_file` (Function)

Walk upward from `cwd` looking for a `.ticket-workspace` file.

### `find_local_workspace_file_from` (Function)

Walk upward from `start` looking for a `.ticket-workspace` file.

### `WorkspaceSource` (Enum)

The layer that produced the resolved index root — useful for diagnostics.

### `WorkspaceSource` (Impl)

### `resolve_workspace` (Function)

Resolve the active index root using the full resolution chain.

Returns `(resolved_path, source)`.

### `make_relative_path` (Function)

Compute a relative path from `base_dir` to `target`.


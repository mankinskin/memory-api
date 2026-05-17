# workspace

Source: `crates/memory-api/src/workspace.rs`

## Public API

### `TICKET_INDEX_DIR` (Const)

The canonical local ticket store directory name: `.ticket`.

### `working_dir` (Function)

Resolve the current working directory from `cwd` or `PWD`, normalizing Git Bash
paths on Windows.

### `find_local_root` (Function)

Walk upward from the working directory looking for a hidden store directory such
as `.ticket`, `.spec`, or `.rule`.

### `find_local_root_from` (Function)

Walk upward from an explicit `start` path looking for a hidden store directory.

### `resolve_local_root` (Function)

Return the discovered hidden store path or fall back to `<cwd>/<dir_name>`.

### `resolve_local_root_from` (Function)

Return the discovered hidden store path or fall back to `<start_dir>/<dir_name>`.

### `resolve_store_root_from` (Function)

Normalize an explicit repo root, hidden store root, or path inside a hidden
store back to the owning store root. If no matching hidden store exists, keep
the direct path unchanged so callers can still open explicit non-workspace test
stores.

### `WorkspaceSource` (Enum)

Describe whether `.ticket` came from upward discovery or the local default
fallback.

### `WorkspaceSource` (Impl)

### `resolve_workspace` (Function)

Resolve the active `.ticket` store from the current working directory.

Returns `(resolved_path, source)`.

### `resolve_workspace_from` (Function)

Resolve the active `.ticket` store from an explicit starting path.


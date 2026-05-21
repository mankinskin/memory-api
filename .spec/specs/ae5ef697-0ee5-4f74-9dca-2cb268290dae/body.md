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

## Workspace contract

- Repository roots, direct hidden-store roots, and paths inside a hidden store must normalize to one owning workspace root when they refer to the same workspace.
- Nested child workspaces must keep their concrete workspace folder names stable regardless of whether callers enter through the repo root or the hidden `.ticket` directory.
- Public transport layers built on top of workspace resolution must expose concrete workspace folder names rather than internal aliases such as `default` or relative placeholders such as `..`.
- Explicit non-workspace paths remain valid direct fallbacks for tests and isolated stores; normalization must not invent a hidden store that does not exist on disk.

## Workspace fixture strategy

The workspace helpers in this module define the entry-point equivalence classes that downstream ticket, HTTP, and MCP contracts rely on. Validation therefore needs to vary both the path a caller starts from and the workspace topology around that path, not just the final resolved store.

The child-workspace dependency spec at `ticket-api/workspaces/ancestor-dependency-visibility` reuses these same fixture classes for mixed-workspace graph behavior. This workspace spec owns the path-resolution side of that shared matrix.

### Entry-point and topology fixture matrix

| Fixture class | Caller start path | Why it exists | Required outcome |
| --- | --- | --- | --- |
| Local repo-root entry | Workspace repo root that contains `.ticket` | Baseline discovery path used by normal CLI and tool callers | Resolves to the local hidden store and preserves the workspace folder name |
| Direct hidden-store entry | The workspace `.ticket` directory itself | Ensures direct-store callers are equivalent to repo-root callers | Resolves to the same owning workspace as the repo-root entry point |
| In-store descendant path | A path inside `.ticket/` such as `.ticket/tickets/<id>` | Covers callers that start from an internal file or folder | Normalizes back to the owning hidden store instead of treating the nested path as a new workspace |
| Nested child workspace | Child repo root or child `.ticket` inside a parent workspace tree | Distinguishes nearest-child ownership from ancestor discovery | Resolves the child workspace without collapsing back to the ancestor store |
| Explicit non-workspace path | A directory with no matching hidden store | Preserves test-store and scratch-store behavior | Keeps the explicit path as the fallback root instead of inventing a workspace |
| Public alias rejection | Downstream caller reuses an internal alias such as `default` or `..` | Separates internal discovery labels from public identifiers | Public workspace contracts reject the alias and require the concrete folder-name workspace identifier |

### Validation matrix

| Observable surface | Local repo/direct store equivalence | In-store descendant normalization | Nested child selection | Non-workspace fallback | Public alias handling |
| --- | --- | --- | --- | --- | --- |
| `working_dir`, `find_local_root`, `find_local_root_from` | Discover the same hidden store from repo-root and direct-store starts | Not applicable | Prefer the nearest matching child workspace | Return no local root when none exists | Not applicable |
| `resolve_local_root`, `resolve_local_root_from` | Return equivalent hidden-store roots | Preserve the owning store root for nested in-store paths | Return the child store when starting inside the child workspace | Fall back to `<start>/<dir_name>` when discovery fails | Not applicable |
| `resolve_store_root_from` | Canonicalize repo roots and direct hidden-store roots to one owner | Canonicalize `.ticket/...` descendants back to the same owner | Keep child ownership when the child store exists | Preserve the explicit path when no hidden store exists | Downstream callers must not treat aliases as substitute roots |
| Downstream workspace names | Concrete folder names remain stable across entry points | Nested internal paths do not invent new workspace names | Parent and child names remain distinct and reversible | Fallback stores do not claim unrelated workspace names | Public surfaces reject `default` / `..` and only emit concrete folder names |


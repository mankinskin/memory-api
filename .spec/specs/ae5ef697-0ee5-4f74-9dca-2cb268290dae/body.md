<!-- aligned-structure:v1 -->

# Summary

Source: `crates/memory-api/src/workspace.rs`

## Behavior Story

Source: `crates/memory-api/src/workspace.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

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
- Downstream ticket/spec/rule tooling must be able to pass an explicit workspace root and resolve the same owning hidden store that direct hidden-store access would have opened.
- Nested child workspaces must keep their concrete workspace folder names stable regardless of whether callers enter through the repo root or the hidden `.ticket` directory.
- Public transport layers built on top of workspace resolution must expose concrete workspace folder names rather than internal aliases such as `default` or relative placeholders such as `..`.
- Explicit non-workspace paths remain valid direct fallbacks for tests and isolated stores; normalization must not invent a hidden store that does not exist on disk.
- Ticket/spec cross-links and code-reference validation must resolve against the intended nested workspace when callers start from an ancestor repo and explicitly target the child workspace root.
- Shared CLI option naming must reserve `--workspace` as the global spelling for workspace/store selection across ticket, spec, rule, audit, session, and test commands; command-local path-resolution inputs must use distinct names when they serve different purposes.
- Public create-style surfaces must require an explicit concrete workspace path before writing a new entity. Omitted workspace selectors, empty strings, `default`, `.`/`..`, and other transport aliases are invalid for entity creation even when read-only commands still support ambient discovery.

## Downstream CLI contract

- Global CLI `--workspace` selects the target nested workspace and is normalized through the shared memory-api workspace resolver before store access.
- Global CLI `--index-root` remains the explicit hidden-store override and takes precedence over global `--workspace` when both are supplied.
- Command-local options that control code-reference validation roots or source-path relativization must not reuse the same `--workspace` spelling as the global store-selection option.
- Entity-creation CLI commands must fail before opening the ambient store when neither `--workspace` nor `--index-root` is supplied.
- Ancestor-repo callers must be able to run nested-workspace ticket/spec/rule commands without spelling the hidden `.ticket`, `.spec`, or `.rule` directories directly.
- Read/query command paths may register descendant scan roots derived from the shared memory-api workspace helper so ancestor-repo callers can resolve nested child entities through one traversal and skip policy across ticket, spec, and rule stores.
- Root-level `spec refs <id> validate` against a nested workspace must work with one global workspace-selection option and, when needed, a separately named file-resolution root option.
- When `spec refs <id> validate` resolves a nested spec through descendant scan roots, the default code workspace root must be derived from the owning spec workspace rather than the ancestor repo root so validation still points at the correct files without a command-specific fallback path.

## Public creation contract

- MCP create/import/record tools for store-backed domains must model `workspace` as a required concrete path argument and reject omitted, blank, `default`, `.` and `..` values before calling the domain store.
- CLI create/import/record commands for store-backed domains must accept the same concrete path through global `--workspace` and reject ambient creation when the global workspace selector and hidden-store override are both absent.
- Validation must include end-to-end MCP transport tests and CLI process tests that create entities in two sibling workspaces and assert no entity appears in the parent context-engine workspace by accident.
- Tracking ticket: `memory-api/.ticket/tickets/0fdce225-9cef-46ed-92d9-83c852c2d084/ticket.toml`.

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
| Explicit child-workspace root | Child repo root passed explicitly from an ancestor repo command | Covers parent-repo callers that need a child ticket/spec workspace without spelling the hidden store directory | Normalizes to the child hidden store through the same shared resolver used by direct store access |
| Explicit non-workspace path | A directory with no matching hidden store | Preserves test-store and scratch-store behavior | Keeps the explicit path as the fallback root instead of inventing a workspace |
| Public alias rejection | Downstream caller reuses an internal alias such as `default` or `..` | Separates internal discovery labels from public identifiers | Public workspace contracts reject the alias and require the concrete folder-name workspace identifier |

### Validation matrix

| Observable surface | Local repo/direct store equivalence | In-store descendant normalization | Nested child selection | Non-workspace fallback | Public alias handling |
| --- | --- | --- | --- | --- | --- |
| `working_dir`, `find_local_root`, `find_local_root_from` | Discover the same hidden store from repo-root and direct-store starts | Not applicable | Prefer the nearest matching child workspace | Return no local root when none exists | Not applicable |
| `resolve_local_root`, `resolve_local_root_from` | Return equivalent hidden-store roots | Preserve the owning store root for nested in-store paths | Return the child store when starting inside the child workspace | Fall back to `<start>/<dir_name>` when discovery fails | Not applicable |
| `resolve_store_root_from` | Canonicalize repo roots and direct hidden-store roots to one owner | Canonicalize `.ticket/...` descendants back to the same owner | Keep child ownership when the child store exists | Preserve the explicit path when no hidden store exists | Downstream callers must not treat aliases as substitute roots |
| Explicit downstream workspace-root inputs | Normalize parent-repo calls that target a child repo root to the same child hidden store | Reuse the same canonical store root for direct-store and repo-root callers | Keep child selection deterministic even when an ancestor workspace also exists | Preserve scratch paths passed explicitly for isolated tests | Public surfaces still expose concrete folder names only |
| Downstream workspace names | Concrete folder names remain stable across entry points | Nested internal paths do not invent new workspace names | Parent and child names remain distinct and reversible | Fallback stores do not claim unrelated workspace names | Public surfaces reject `default` / `..` and only emit concrete folder names |

## Validation expectations

- Focused automated tests must cover global workspace-root targeting for ticket, spec, and rule CLI store selection.
- Focused automated tests must exercise representative nested-workspace read commands such as `get`, `list`, and `search`, not just helper-level root resolution.
- Focused automated tests must cover the root-level nested-workspace `spec refs validate` path and prove that store selection and file-resolution options do not share one ambiguous flag name.
- When a CLI discovers descendant scan roots dynamically, validation must prove that full-text search stays in sync with `get` and `list` for the same nested workspace fixture.
- Shared workspace-spec updates and ticket acceptance criteria must stay aligned with those automated tests whenever the option contract changes.

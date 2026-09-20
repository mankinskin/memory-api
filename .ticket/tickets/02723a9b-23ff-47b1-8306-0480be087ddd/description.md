# Problem

Two ticket discovery paths are still unreliable for nested child workspaces.

1. From the repo root, `ticket search "Persist"` does not find `deeeb26d-cb73-46c5-bf2a-1778caa7f82a`, even though that ticket exists on disk under `memory-api/.ticket` and is immediately discoverable when `--index-root memory-api/.ticket` is supplied.
2. In the running ticket-viewer root route, clicking the `in-review` state chip before the initial unfiltered list settles can leave the sidebar showing the stale full list because the earlier response overwrites the later filtered response.

# Goal

Make nested-workspace ticket discovery reliable from the common repo-root flows:

- `ticket search` should surface tickets owned by related child workspaces when invoked from the repo root store.
- ticket-viewer list filtering should not be overwritten by stale in-flight responses, so the active filter deterministically controls the rendered rows.

# Acceptance Criteria

- Running `ticket search "Persist"` from the repo root returns `deeeb26d-cb73-46c5-bf2a-1778caa7f82a` without requiring an explicit `--index-root` override.
- The viewer root route can apply the `in-review` filter immediately after load without ending in the stale unfiltered list.
- Focused regression coverage exists for the repo-root CLI search path and the viewer stale-response filtering race.

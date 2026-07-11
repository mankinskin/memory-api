## G-E — De-submodularization (GATED behind G-A..G-D)

Replace nested git submodule links (context-engine → memory-viewers → memory-api/viewer-api) with real dependency-level imports / install-path deps on remote releases. Treat context-engine as a standalone workspace consuming released artifacts, not a "collection of repositories" coupled by submodule resolution.

## Scope
- Remove submodule links; encode dependencies as package/version deps or documented install paths on remote release branches.
- Keep context-engine as a collection repo FOR NOW; future work dissolves it toward individual repositories with clear consumers.

## Gating
This ticket depends_on G-A, G-B, G-C, G-D. Do not start until the content-materialization workstreams land — de-submodularizing mid-content-fill would destabilize the workspace.

## Acceptance criteria
- Submodule links replaced with real dependency declarations / install paths.
- Build + tooling still resolve without `git submodule update --recursive`.
- context-engine remains buildable as a collection workspace.
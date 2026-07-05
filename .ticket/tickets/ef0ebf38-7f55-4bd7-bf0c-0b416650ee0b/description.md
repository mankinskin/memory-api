# Problem

Current child-workspace resolution across the CLI tools is inconsistent even though store selection already uses a shared workspace-root/index-root resolver.

Observed from the repo root:

- `spec refs <id> validate` can now probe descendant `.spec` stores, but `spec get`, `spec tree`, and `spec search` still miss the same child-owned spec.
- `ticket get` and `ticket search` still operate only on the initially selected `.ticket` store and do not discover child workspaces.
- `rule` does aggregate descendant `.rule` workspaces, but it does so through a rule-specific recursive walker instead of the shared `memory-api::workspace` helper.
- The traversal and skip policy has already diverged between the shared workspace helper and the rule-specific implementation.

This leaves operator behavior inconsistent across `spec`, `ticket`, and `rule`, and keeps the same complexity in multiple places.

## Goal

Use one shared child-workspace discovery and composition path for all three CLI tools so ancestor-repo callers see consistent read behavior while mutating commands remain explicit about their target workspace.

## Scope

- move descendant hidden-store discovery and composition into shared `memory-api::workspace` utilities;
- make `spec`, `ticket`, and `rule` read commands use the same child-workspace aggregation logic;
- remove the rule-specific recursive workspace walker in favor of the shared helper;
- replace the `spec refs`-only fallback with the same generic resolution path used by the rest of `spec`;
- keep mutating commands scoped to the explicitly selected store unless a command already requires an explicit target root.

## Nearby tickets

- `07836f41-7fa5-4e41-8411-1c7cf8aeee75` covers the ticket-cli-only read-path symptoms.
- `59d96577-09a8-44a7-b0ea-3d51b3a6fb05` covers the spec-cli and spec-mcp-only read-path symptoms.
- `050c5441-1d3a-46bc-9748-cfb7030a93bd` previously added nested rule workspace support with rule-specific traversal logic.

This ticket should provide the shared implementation contract that lets those tool-specific slices converge instead of diverging further.
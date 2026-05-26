## Parent–child configuration

- Parent workspaces declare their child stores in `rule-targets.yaml` (`imports:`) and through registered scan roots in each store.
- A nested workspace's local `rule-targets.yaml` is consulted by `spec sync-generated` when the spec lives in that workspace; the parent's targets are not implicitly inherited.
- `<x>-cli ... --workspace-root <path>` forces the resolver to a specific root and is the supported way for an ancestor checkout to target a nested workspace explicitly.
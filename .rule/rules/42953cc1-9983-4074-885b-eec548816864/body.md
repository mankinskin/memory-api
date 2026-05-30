### Planned acceptance criteria for the next slice

- Specs can declare generated `body.md` and named section artifacts through an explicit artifact-to-target mapping.
- Syncing generated spec artifacts uses a spec-owned workflow rather than writing files behind `spec-api`'s back.
- The design keeps `spec.toml` authored and local while generated prose moves into canonical rules and target outputs.
- At least one real spec migration validates the authoring and regeneration workflow end to end.
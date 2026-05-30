## Health interaction

- Health checks read the schema and report `actionable-with-deps`, `missing-required-field`, or `invalid-transition` findings without mutating state.
- A schema change that tightens requirements re-runs health on existing entities; pre-existing entities that violate the new requirements are reported as findings and not silently demoted.
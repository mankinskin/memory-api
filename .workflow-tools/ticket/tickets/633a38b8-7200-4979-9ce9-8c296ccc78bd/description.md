## G-A — Spec-contract v2

Extend the `aligned-structure:v1` spec template into a real contract shape. Every spec must declare:

1. **Motivation ("why")** — the user requirement / behavior need this spec satisfies, with optional feedback links explaining origin.
2. **Dependent expectation** — an explicit "if this spec is implemented, dependents can rely on X" clause (the contract dependents build against).
3. **Guards** — a declared test collection (test-api ValidationSpec ids) that gate the spec. Spec `verified` state is COMPUTED from latest execution outcomes, never hand-set.
4. **Positions** — per referenced code symbol/path: status ∈ {implemented, partial, not-implemented, deprecated} with a code_ref.
5. **Governing-rule requirement** — link to the PolicyRule(s) that must introduce/explain this spec in-session (see G-C).

## Deliverables
- A spec authored in memory-api describing spec-contract v2 (the meta-contract).
- A workflow policy rule explaining idiomatic, practical use of the template.
- The `aligned-structure` template updated to v2 fields.
- First dogfood: enrich spec 8c880efc (session bootstrapping) "Provided Surface Contracts" + "Required Validation" against v2.

## Acceptance criteria
- spec-contract v2 template lands with the five required sections.
- `verified` is computed from guard executions (design + validation, implementation may follow).
- At least one existing spec is migrated to v2 as proof.
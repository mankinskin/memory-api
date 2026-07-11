# Summary
Upgrade the `aligned-structure:v1` spec template into a real contract (`v2`) so specs stop being module-name stubs and become dependable, verifiable contracts consumed by session construction (epic effba966).

# Motivation (why)
Store discovery (2026-07-10/11) found ~60 specs, nearly all `draft`, titled after modules (`routes`, `store`, `error`) with no positions, guards, or motivation. Dependents cannot rely on a spec that states no expectation and declares no passing tests. This contract makes "what can I rely on if this spec is implemented?" a first-class, testable question.

# Required contract sections (v2)
1. Motivation → user-requirement + optional feedback links (origin of the requirement).
2. Dependent expectation — explicit "if implemented, dependents can rely on X".
3. Guards — declared test-api ValidationSpec ids; `verified` state is COMPUTED from latest execution outcomes, never hand-set.
4. Positions — per referenced code symbol/path: {implemented | partial | not-implemented | deprecated} + code_ref.
5. Governing rule — link to the PolicyRule(s) that must introduce/explain this spec in-session (see rule-introduces-spec).

# Provided Surface Contracts
- A `v2` template definition with the five required sections.
- Computed `verified` semantics: spec is verified iff every declared guard's latest execution passed.

# Required Validation
- Template presence check: a v2 spec exposes all five sections.
- Computed-state check: toggling a guard execution outcome flips `verified`.
- Migration proof: at least one existing spec (target: 8c880efc session bootstrapping) migrated to v2.

# Related Implementation Tickets
- Ticket 633a38b8 (G-A) under epic 3be95a71, which depends_on session-construction epic effba966.

# Background Knowledge References
- Existing template marker: `aligned-structure:v1` (see spec 8c880efc body).
- Guard mechanism: test-api ValidationSpec / ValidationExecution.
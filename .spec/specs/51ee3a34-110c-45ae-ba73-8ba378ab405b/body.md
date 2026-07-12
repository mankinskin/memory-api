<!-- aligned-structure:v2 -->

# Summary

Require that every spec is introduced and explained in-session by a governing PolicyRule, with presentation conditioned on the spec's implementation status. Wires into session construction (epic effba966) so spec availability is legible and policy coverage is forced for specced features.

## Motivation ("why")

Specs today are invisible at runtime — nothing guarantees an agent is told a spec exists, whether it is implemented, or how to use it idiomatically. This lets agents assume unavailable behavior and lets specced features ship without governing policy. Requiring a rule to introduce each spec closes both gaps.

## Dependent expectation

If this spec is implemented, dependents can rely on every spec being introduced in-session by a governing PolicyRule, where the presentation status is dynamically conditioned into implemented, partial-with-gaps, or coming-soon based on the spec's computed readiness.

## Guards

The verification of this specification contract is gated by:
- `val-rule-introduces-spec-coverage-validation` (verifies that specced features have a governing rule)
- `val-spec-status-presentation-validation` (asserts the status-conditioned branch matches the spec position status)

## Positions

- Obligation and status conditioning logic: `implemented` at [./memory-api/crates/rule-api/src/obligation.rs](./memory-api/crates/rule-api/src/obligation.rs)
- Presentation mapping logic: `implemented` at [./memory-api/crates/session-api/src/presentation.rs](./memory-api/crates/session-api/src/presentation.rs)

## Governing-rule requirement

This specification is governed and introduced by:
- [shared/instructions/spec-system/spec-system-guidance/spec-authoring-workflow/structure-the-spec/l52](shared/instructions/spec-system/spec-system-guidance/spec-authoring-workflow/structure-the-spec/l52)
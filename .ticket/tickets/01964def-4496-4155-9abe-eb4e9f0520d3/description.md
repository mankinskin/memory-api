# [bench] Scale-sensitive latency fixtures + per-operation budgets

## Goal

The parent exists because `ticket get` once took 96–107s. The current benchmark measures ~347ms on a 1-ticket fixture under a 2s budget — it cannot reproduce or flag that regression class. Add a scale axis and **per-operation end-user budgets**.

## Resolved decisions

- **D9 — budgets:** **per-operation** budgets sized so **no end-user waits longer than reasonable** for a given command. Measure and budget **each end-user call** (per operation, and per transport where it materially differs). These are real latency contracts, not just coarse tripwires.
- **D6/D10 — runtime:** scale runs are **large-lane** (on-demand/debounced) via test profiles `2dada4b7`; small N stays in the fast lane.

## Scope

- Parameterize benchmarks over store size (e.g. N ∈ {1, 1k, 10k} entities) using the synthesized fixture (`9138f4e7`).
- Define a per-operation budget table (per transport where relevant) with documented rationale tied to acceptable end-user wait.
- Re-validate the `ticket get` latency fix (`23f4e2eb`) at scale; document the resulting budget against the representative fixture.
- Tag scale cells with their profile (fast vs large).

## Brutally-honest expectations

- Include a **synthetic 100s-class regression** test proving the budget assertion actually fails when latency blows up — if it doesn't fail, the budget is wrong.
- If a real operation exceeds its reasonable budget at scale, **leave the benchmark failing** and open a performance fault; do not inflate the budget to pass.

## Acceptance criteria

- [ ] Benchmarks run at multiple store sizes; size recorded as a dimension.
- [ ] A synthetic 100s-class regression is demonstrably flagged by the budget assertion.
- [ ] Per-operation (and per-transport where relevant) budgets are documented with rationale.
- [ ] `ticket get` re-measured at scale; documented budget holds against the representative fixture.

## Relationship / traceability

- Depends on representative fixture `9138f4e7` and benchmark model `2b0f31e5`.
- Re-validates latency fix `23f4e2eb`; profiled by `2dada4b7`.

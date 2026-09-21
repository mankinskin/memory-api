# Goal
Research and codify one repository workflow for end-to-end tool benchmarking that measures latency, throughput, and load behavior, records executable anchors, and produces durable metrics/evidence summaries.

## Why this work exists
Benchmarking work is split across Criterion benches, ad hoc perf harnesses, and newer `test-api` / `log-api` execution capture plans. We need one policy that tells contributors how to define the benchmark surface, run it under load, capture evidence, and summarize results in a reusable way.

## Research questions
- Which benchmark surfaces should be first-class: CLI, HTTP, MCP, in-process library, viewer/server workflows?
- How should executable anchors be defined so runs are repeatable and attributable across transports and environments?
- What should flow into `test-api` executions versus `log-api` artifacts versus benchmark-specific summaries?
- Should the repository converge on one shared harness/library and fixture workspace for load generation and metrics capture?
- Where are the current gaps between Criterion, custom perf runners, and transport-layer tests?

## Starting anchors
- existing benchmark tracker: `a0bc8bd8-3fe0-4768-a895-b1bacee42759` (`[memory-api][test] Unified validation & benchmark surface in test-api (tracker)`)
- existing completed tickets: `7a524627-bb48-47c8-a3d8-9c8b9303f0f3` (`Validation runner harness — cargo test/bench to executions + log capture`), `d760a9bb-b970-4a97-8e38-fb4d78a5ea10` (`Capture structured tracing for Criterion and Rust perf harnesses`), `ff6637f5-01f6-46c3-b727-e1a19ee0f202` (`Capture profiling timings through logs and journals`)
- draft spec seed: `76da5f2d-cea9-49d9-b223-730a0c2a5d6b` (`Transport-layer e2e matrix and benchmark strategy`)

## Deliverables
- benchmark taxonomy and transport matrix
- recommended shared harness/library and fixture strategy, or explicit reasons not to unify
- executable-anchor schema for commands, environments, load profiles, and result artifacts
- follow-up implementation tickets and any required spec updates

## Validation expectations
The next session should finish with a concrete benchmark policy outline plus at least one proposed executable anchor shape validated against an existing bench or transport test.

## Research snapshot (2026-07-09)

- Durable artifact landed: spec `76da5f2d-cea9-49d9-b223-730a0c2a5d6b` now defines an executable-anchor contract in addition to the existing transport-cell matrix.
- Canonical distinction: `cell_id` identifies one domain-operation-transport cell, while an executable anchor owns the runnable command, environment shape, load profile, fixture profile, evidence targets, and budget policy.
- Grounded examples are now captured from existing evidence:
	- `vt-cross-domain-matrix` -> `cargo test -p memory-matrix`
	- `vt-bench-matrix` -> `cargo run -p memory-matrix --bin bench-matrix`
- Current mismatch list is explicit: fixture-profile identity is not yet canonical in execution records; benchmark runs do not yet require companion runtime-log/profile artifact pointers; environment vocabulary still needs normalization across Criterion, subprocess, and direct-dispatch runs.
- Validation completed: `spec.exe refs 76da5f2d-cea9-49d9-b223-730a0c2a5d6b validate --workspace-root memory-api --toon` returned `valid: true`.

## Decomposition note

The benchmark slice does not need a new parallel tracker yet. Existing `memory-api` children already cover the likely implementation surfaces in `memory-matrix`, `test-api`, and log capture; the next session should either create one narrowly scoped executable-anchor integration ticket if the current children are insufficient, or wire the new anchor contract into the existing matrix and bench tickets directly.

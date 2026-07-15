# [design][test] Browser and TypeScript result integration with test-api

# Goal

Choose and document how Playwright, TypeScript-authored checks, browser artifacts, and wasm-pack browser tests become first-class ValidationExecution and BenchmarkExecution evidence instead of remaining isolated npm/stdout reports.

This is a design ticket. Implementation belongs in follow-up tickets and the viewer-platform umbrella `956485ad`.

# Decisions required

- Select the runner boundary: invoke repository-native npm/Playwright/wasm-pack commands from the validation harness rather than embedding a JS runtime unless evidence demonstrates embedding is necessary.
- Select the result adapter: prefer a thin structured reporter/adapter that emits the test-api schema over scraping human-oriented console output.
- Define provenance for source file, test ID/title, project/browser profile, command, commit, correlation ID, transport, retry, and artifact paths.
- Map Playwright statuses and infrastructure failures to passed/failed/blocked without turning retries into false passes.
- Map screenshots, traces, videos, frontend/backend logs, environment manifests, and benchmark output to durable artifact references.
- Define fast PR, release-browser, nightly performance/soak, and hardware/on-demand profiles.
- Represent wasm-pack browser tests and benchmark samples without conflating unit-test success with performance-budget success.

# Acceptance criteria

- [ ] A short design note chooses the runner and adapter architecture with rationale and rejected alternatives.
- [ ] The provenance model covers correlation ID, source test, environment profile, retry, transport, and artifact identity.
- [ ] Playwright, wasm-pack, and benchmark outcomes have explicit mappings to test-api records.
- [ ] Follow-up implementation tickets are created and linked to `956485ad`.
- [ ] Artifact retention and missing-capability/blocked behavior are specified.
- [ ] The design composes with `9202bc21` correlated logs and `0556ed59` CI lanes.

# Work steps

1. Inspect test-api execution/benchmark schemas and existing reporter extension points.
2. Prototype one Playwright structured result payload and one wasm-pack payload.
3. Compare thin reporter, generic subprocess adapter, and embedded-runtime alternatives.
4. Specify provenance and artifact lifecycle.
5. Create implementation tickets and link exact validation guards/specs.

# Non-goals

- Replacing Playwright with a custom browser driver.
- Parsing human console output when a structured reporter is available.
- Running all browser/performance work in the fast PR lane.
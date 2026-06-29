# [test-api][ci] Test profiles + CI lanes — fast-on-push vs. large-on-demand

## Goal

Provide a **test-profile** mechanism that selects which validation/benchmark cells run, and wire two CI lanes (D6, D10):

- **fast** — runs on every push: in-process matrices, small fixtures, quick budgets.
- **large/on-demand** — runs on demand or once with a long debounce after the last push: real-subprocess/TS e2e, scale fixtures, whole-corpus backfill, browser tests.

## Scope

- A profile selector (env/flag/config) the matrices and runner harness honor, tagging each cell with the profile(s) it belongs to.
- Default profile assignment per cell: in-process transport cells + N=1 scale → fast; subprocess/TS/browser + N≥1k scale + corpus backfill → large.
- CI lane definitions: fast on push; large on-demand and/or debounced-after-last-push (define the debounce window).
- Profiles must NOT hide failing tests — an excluded test is reported as "not run in this profile", never as passed (ties into the brutally-honest principle).
- TOON/JSON summary distinguishes not-run-in-profile from passed/failed/blocked.

## Acceptance criteria

- [ ] A documented profile flag runs the fast subset quickly and the large subset fully.
- [ ] Each cell declares its profile membership; the store/index records which profile produced a run.
- [ ] CI runs fast on push and large on-demand/debounced, per documented config.
- [ ] Excluded cells are surfaced as not-run, never silently counted as passing.

## Open considerations

- Exact debounce window/trigger for the large lane (e.g. once N minutes after the last push, or manual dispatch only).

## Relationship / traceability

- Organizes the suites from the transport matrix (`387843e4`), scale fixtures (`01964def`), and backfill (`274c5119`).
- Consumes the store-index (`90de77b1`) for per-profile reporting.

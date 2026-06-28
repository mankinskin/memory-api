# [test] Cross-domain operation test matrix — get/search/CRUD/move/scan × all domains

## Goal

Implement an end-to-end test matrix that exercises the basic operations of **every memory domain** against the representative fixture, recording each run as a `test-api` `ValidationExecution` with a duration.

## Concrete matrix

Domains (rows): `ticket`, `spec`, `rule`, `audit`, `session`, `test`, `doc`, `log`.
Operations (columns): `get`, `search`, CRUD (`create`/`read`/`update`/`delete`), `move`, `scan`.

| Domain | get | search | create | update | delete | move | scan |
|---|---|---|---|---|---|---|---|
| ticket | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| spec | ✓ | ✓ | ✓ | ✓ | ✓ | ✓* | ✓ |
| rule | ✓ | ✓ | ✓ | ✓ | ✓ | ✓* | ✓ |
| audit | ✓ | ✓ | ✓ | ✓ | ✓ | ✓* | ✓ |
| session | ✓ | ✓ | ✓ | ✓ | ✓ | ✓* | ✓ |
| test | ✓ | ✓ | ✓ | ✓ | ✓ | ✓* | ✓ |
| doc | ✓ | ✓ | ✓ | ✓ | ✓ | ✓* | ✓ |
| log | ✓ | ✓ | ✓ | ✓ | ✓ | ✓* | ✓ |

`✓*` move = covered once the generic move kernel (`0a510279`) lands; until then, assert the domain is wired for move-preflight or mark the cell `blocked` with a recorded reason (no silent skips).

## Scope

- A data-driven harness (table of `domain × operation` cases) so adding a domain or operation is a row/column edit, not new boilerplate.
- Each case runs against a materialized copy of fixture `026b2eb6`, asserts correctness, measures wall time, and records a `ValidationExecution` (outcome + `duration_ms`, linked to a `ValidationSpec` per `domain.operation`).
- `blocked` outcomes (e.g. unsupported move) are recorded with a reason, never skipped.

## Acceptance criteria

- [ ] Every `domain × operation` cell runs and produces a recorded `ValidationExecution` with a duration (or a `blocked` execution with a reason).
- [ ] Correctness assertions pass for get/search/CRUD/scan on all 8 domains.
- [ ] Adding a new domain or operation requires only a new matrix row/column + spec, no new harness code.
- [ ] The suite runs via a single documented command and writes executions into the workspace `.test` store.

## Relationship / traceability

- Depends on the execution timing model and the fixture `026b2eb6`.
- Move cells depend on `0a510279` / `21e6c015`.

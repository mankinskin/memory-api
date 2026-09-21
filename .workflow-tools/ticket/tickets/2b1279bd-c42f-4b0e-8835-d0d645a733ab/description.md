Refactor memory-api shared storage/index/search APIs to domain-neutral semantics (`entity`, `store`, `workspace`) and isolate ticket-only behavior from shared storage internals.

Implementation plan:
1. Introduce neutral names for shared index operations and table abstractions.
2. Keep temporary compatibility aliases for current ticket-shaped method names.
3. Move ticket-specific semantics out of shared memory-api layers into ticket-api.
4. Update rule-api and spec-api to consume neutral shared APIs directly.
5. Mark ticket-shaped aliases deprecated with a removal gate tied to tracker phase E.

Design constraints:
- no behavior regressions for scan/search/index workflows during transition
- no required big-bang migration; adapters must support mixed old/new call sites
- shared layers must not encode ticket-only naming or guidance

Validation evidence:
- focused tests for memory-api storage/index/search modules
- focused tests for rule-api/spec-api scans and CRUD paths using new surface
- compile and smoke checks proving ticket-api remains operational through aliases

Acceptance criteria:
- shared crates expose neutral API names and docs
- rule-api/spec-api/ticket-api compile against neutral surface (aliases allowed temporarily)
- tests prove open/scan/query equivalence across ticket/spec/rule stores
- migration notes include alias-removal criteria and compatibility window

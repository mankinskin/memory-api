>Frontend migration is in progress; root-route mixed-workspace history/files flow is validated and asset follow-up investigation continues.

## Dependency Rationale Update (2026-05-18)

This rollout now depends_on tracker c33419c2 because remaining work is primarily regression completeness hardening: fixing asset follow-up selection/fetch behavior, adding endpoint ownership matrix coverage, and normalizing release E2E assumptions away from ambient workspace=default defaults. The dependency keeps closure criteria explicit and prevents partial rollout completion before mixed-workspace invariants are verified end-to-end.
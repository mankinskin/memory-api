Project tracker for the audit quality backlog: hardening repository quality audits so ticket/spec/graph health produce more actionable signal.

## Rolled-up work

- `95d4f986` [audit-api] Include dependency convergence findings in default repo audit — **done**.
- `a762448e` [audit-api] Require every ticket to participate in dependency graph — **done**.
- `5ad5ab28` [ticket-api][audit-api] Strengthen canonical ticket health validation — **active track**. This is the next major slice and the parent of the future phase-by-phase child tickets described in its body.

## Decomposition policy

Do not expand `5ad5ab28` into one mega-implementation. As each phase is reached (orphan parity, schema reconciliation, traceability/evidence checks, board-ownership health, migration remediation), split it into a child ticket that `depends_on` the prior phase and rolls up under this tracker. The tracker closes when all child phases are done.

## State

Started `new`; remains `new` until its child/leaf tickets converge, per the tracker dependency convention (a tracker depends_on its children and closes when they are done).
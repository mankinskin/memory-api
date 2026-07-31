## Problem

memory-viewers/ticket-viewer/frontend/dioxus/e2e-release/shared/common-viewer-suite.ts imports from an incorrect relative path:

```
import { loadAndInspectViewer } from '../../../../../viewer-api/viewer-api/frontend/dioxus/e2e/test_apis';
```

This resolves to a nonexistent path (one directory level off), so the module fails to load and the release E2E suite (`ticket-viewer.release.spec.ts`, which registers via `registerCommonViewerSuite`) executes 0 tests instead of running. Pre-existing defect, confirmed still failing on 2026-07-30.

## Evidence / reference fix

The sibling suite `structured-parts.release.spec.ts` in the same `e2e-release/` directory loads correctly and passes 3/3. Use its import depth into `viewer-api/viewer-api/frontend/dioxus/e2e/test_apis` (or whatever helper it uses) as the reference for the correct relative path from `e2e-release/shared/common-viewer-suite.ts`.

## Acceptance Criteria

1. `memory-viewers/ticket-viewer/frontend/dioxus/e2e-release/shared/common-viewer-suite.ts` imports `test_apis` (or equivalent) from a path that resolves to an existing file.
2. Running the ticket-viewer release E2E suite (`npm run test:e2e:release` in memory-viewers/ticket-viewer/frontend/dioxus) actually executes the tests registered by `registerCommonViewerSuite(TICKET_VIEWER)` and `registerDioxusThemeSuite(TICKET_VIEWER)` — the fix must be verified by observing tests run and pass, not merely by the import path resolving syntactically.
3. No regression to the passing `structured-parts.release.spec.ts` suite (3/3 still passes) in the same run.
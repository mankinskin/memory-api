## G-B retro-fix 1 — Typed error for the interoperability-contract violation

Replace the stringly-typed journal contract marker with a typed error variant.

Current state (from the artifact-routing interoperability track): the journal edge reuses `MoveError::Domain` with the stable string marker `MoveJournal::INTEROP_CONTRACT_MARKER` ("interoperability contract violation for move-journal") to avoid exhaustive-match churn across the five domain adapters.

## Task
- Introduce a dedicated typed variant (e.g. `MoveError::InteroperabilityContract { .. }`) carrying the artifact class and the gap detail, instead of matching on a string marker.
- Update the five domain adapters' exhaustive matches (audit, rule, session, spec, ticket) accordingly.
- Preserve behavior; keep tests green.

## Files
- memory-api/crates/memory-api/src/storage/move_kernel_types.rs (marker + contract methods)
- memory-api/crates/memory-api/src/storage/move_kernel/internal.rs (persist_journal enforcement)

## Lineage
First concrete instance of the G-B typed-error policy. Anchors: INTEROP db9bad13, TRACKER 6e72756f.
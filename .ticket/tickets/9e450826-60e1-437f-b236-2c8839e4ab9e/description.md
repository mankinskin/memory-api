## Objective
Turn the Track 1 research brief into an implementation-ready design for 7ef3f8db.

## Required Plan
Define model/API changes, migration-compatible representations, validation algorithm order, atomic reload/cache invalidation boundary, governing-rule artifact, focused tests, and exact target files. Preserve declared rework/replan loops and cancellation exception.

## Done
A reviewed implementation plan resolves all Track 1 decisions without performing production edits.


## Implementation Plan (2026-08-06)

### Model changes
Add a strict source model to `memory-api/crates/memory-api/src/model/schema.rs`: `parent: Option<String>` names the sole parent type ID; `LifecycleCategory` is the closed `plan | act | verify` enum; and `LifecycleNode { id, category }` replaces the lifecycle meaning presently implicit in `states`. Retain `states` as the legacy-compatible serialized state-name list, add an optional `lifecycle: Option<LifecycleGraphSource>`, and represent a derived refinement as `LifecycleRefinement { inherited_node, nodes, transitions }` that replaces one inherited node with a same-category contained subgraph. Resolution materializes a `ResolvedLifecycleGraph` with every concrete node carrying a category, parent chain, directed transitions, and terminals; a child never field-merges a parent state object.

This source syntax keeps old TOML/JSON state lists representable for Track 2 and Track 5 while making inheritance/refinement explicit and diagnosable. Diagnostics identify the declaring source path, referenced parent/node, and the resolved parent chain for missing parent, cycle, duplicate type ID, and invalid refinement failures.

### Registry changes
Make `memory-api/crates/memory-api/src/model/schema_registry.rs` own immutable `RegistryGeneration` snapshots containing raw sources, resolved schemas, source/ancestor hashes, and a typed `ResolvedRegistryManifest { model_version, sources, schemas, graphs }`. Expose `SchemaRegistry::reload_from_dir(&self, dir: &Path) -> Result<RegistryVersion, StorageError>` as the explicit reload API, backed by `Arc<RwLock<RegistryGeneration>>`; readers clone or borrow the current immutable generation, while reload builds and validates a complete candidate outside the write lock and acquires the write lock only to replace the whole generation.

`memory-api/crates/ticket-api/src/model/schema_registry.rs` becomes the ticket adapter: it collects ticket built-ins and ticket-local external source paths, then delegates candidate construction and swap to the shared registry instead of `load_dir` mutating the live `BTreeMap` file by file. The version is a monotonic `u64` generation encoded as the manifest `model_version` string `registry-v<generation>`; a failed reload does not advance it. Parent lookup, cycle detection, and per-kind type-ID uniqueness all run while building the candidate before any live snapshot changes.

### Validation order
For each explicit load/reload, validate in this order:

1. Parse every selected source into the shared source model and retain each source path/content hash.
2. Partition by schema kind and reject duplicate type IDs within the partition, reporting colliding source paths deterministically.
3. Resolve each declared parent within the same kind/namespace; reject missing parents.
4. Detect cycles over the now-resolved single-parent chains.
5. Resolve inherited nodes/refinement tunnels and validate node identity/category containment plus directed-edge legality.
6. Validate that every concrete lifecycle-enabled work-item schema has at least one directed path visiting `plan`, then `act`, then `verify`.
7. Require exactly one `plan` root with no inbound lifecycle edge.
8. Require each terminal to have no outgoing edge, and require `cancelled` to be a derived `verify` terminal leaf with the sole direct-from-any-category exception.

The refinement-resolution check precedes category-path/root/terminal checks because those checks must run against the resolved graph, not an incomplete source graph. Legacy schemas with no `lifecycle` remain parseable and are represented as legacy/unclassified rather than being silently assigned categories; Track 5 owns activating the new lifecycle for migrated records.

### Lifecycle / relation disentanglement
Move `TicketStore::enforce_dependency_progress` out of `TicketStore::resolve_transition_path` into a separately named `TicketStore::enforce_dependency_progress_gate`. `TicketStore::update` calls the dependency gate after loading the ticket and before lifecycle-path resolution; the resulting sequence is `load indexed ticket -> enforce dependency-progress relation gate -> resolve directed lifecycle path -> validate workflow history -> persist transition`.

The dependency policy retains existing observable behavior for unresolved `depends_on` edges, cancellation, on-hold, and non-forward transitions; the change makes relation policy independently callable and prevents lifecycle traversal from reading relation edges. Lifecycle validation changes from the current bidirectional interpretation to declared directed edges, with only the specified rework/replan/cancellation exceptions.

### Atomic reload / cache boundary
The atomic snapshot contains raw parsed sources, resolved schemas, parent chains, per-source hashes, resolved-schema cache entries, the resolved-registry manifest, and the monotonic registry version. Candidate building recomputes a resolved schema whenever its own source hash or an ancestor hash differs; failed parse/validation/cache construction retains the complete prior snapshot and version.

Catalog indexes and client cache versions are named manifest/version seams in Track 1 but remain owned by Track 3 and Track 4 respectively. Track 1 publishes immutable manifest data and the generation/version atomically so Track 3 can derive catalog indexes and Track 4 can advertise/cache client versions from one committed generation, without implementing either owner’s output or transport.

### Cross-store impact
`spec-api` and `rule-api` adopt the shared source/resolved lifecycle types and shared atomic `SchemaRegistry` generation API, while retaining separate `spec`, `rule`, and `ticket` graph collections and independent type-ID namespaces. Track 1 makes lifecycle categories available and validates them whenever a spec or rule schema opts into `lifecycle`; Track 1 does not rewrite existing spec/rule built-in source data or turn on category enforcement for legacy spec/rule records. Track 3/Track 5 own source conversion and migration activation respectively.

### Compatibility preserved for Track 2 and Track 5
Keep the legacy flat `states`/`transitions` representation deserializable alongside the new optional lifecycle source model so Track 2 can add the shared TOML/JSON parser without a second incompatible model. Preserve source paths, deterministic per-kind type-ID diagnostics, parent/source hashes, and the `registry-v<generation>` manifest version so Track 2 can compare equivalent TOML/JSON resolved manifests and Track 5 can identify the model cutover and leave pre-cutover records exempt until individual migration. Do not convert shipped TOML schemas, delete legacy fields, or reinterpret historical transition history in Track 1.

### Target files
- `memory-api/crates/memory-api/src/model/schema.rs` — add source/resolved inheritance, lifecycle-category, refinement, terminal, and directed-transition model primitives.
- `memory-api/crates/memory-api/src/model/schema_registry.rs` — implement candidate construction, validation, immutable generations, hashes, cache entries, manifest, and atomic reload/swap.
- `memory-api/crates/memory-api/src/error.rs` — add deterministic schema-load and lifecycle/refinement diagnostic variants.
- `memory-api/crates/ticket-api/src/model/schema_registry.rs` — replace ticket-local insert-as-you-go loading with the shared atomic reload adapter.
- `memory-api/crates/ticket-api/src/model/default_schema.rs` — adapt delivered-schema registration to the shared generation API while preserving legacy source representation.
- `memory-api/crates/ticket-api/src/storage/store.rs` — separate the dependency-progress relation gate from lifecycle path resolution and make lifecycle path traversal directed.
- `memory-api/crates/ticket-api/src/storage/tests/update_regression_tests.rs` — cover directed update/reopen rejection and preserved state behavior at the ticket update boundary.
- `memory-api/crates/ticket-api/src/storage/tests/workflow_tests.rs` — cover the independently invoked dependency-progress gate and its preserved relation-policy behavior.

### Focused tests
- In `memory-api/crates/memory-api/src/model/schema.rs`, add `directed_lifecycle_edges_reject_reverse_traversal`, `category_path_requires_plan_act_verify`, `resolved_lifecycle_requires_exactly_one_plan_root`, `terminal_nodes_reject_outgoing_edges`, and `cancelled_is_verify_terminal_reachable_from_each_category`.
- In `memory-api/crates/memory-api/src/model/schema_registry.rs`, add `resolves_single_parent_refinement_with_inherited_categories`, `reload_rejects_missing_parent_cycle_and_duplicate_type_id`, `failed_reload_retains_prior_generation`, and `ancestor_hash_change_invalidates_resolved_cache_and_manifest`.
- In `memory-api/crates/ticket-api/src/model/default_schema.rs`, add `delivered_legacy_schemas_remain_loadable_without_lifecycle_activation`.
- In `memory-api/crates/ticket-api/src/storage/tests/update_regression_tests.rs`, add `update_rejects_reverse_lifecycle_transition`.
- In `memory-api/crates/ticket-api/src/storage/tests/workflow_tests.rs`, add `dependency_progress_gate_runs_before_directed_lifecycle_resolution` and retain/update `update_guards_transition_ahead_of_dependency_state` as the relation-policy regression seam.

### Scope boundaries
NOT in Track 1:

- Dual TOML/JSON parsing, lexical mixed-format ordering, and TOML/JSON fixture parity (Track 2).
- Generated catalogs, shipped-ticket JSON conversion, deleting shipped TOML, catalog-index generation, and catalog publishing (Track 3).
- CLI, MCP, HTTP, VS Code, or other client cache-version transport/invalidations (Track 4).
- Legacy-record migration, active-record preflight, historical exemption removal, remediation approvals, and cutover execution (Track 5).
- Release-wide validation matrix and corrective migration batches (Track 6).
- New rule entities or lifecycle-category constraints on relation edges.

## Planning Output Obligation

This planning ticket is not complete until it has written the following into its implementation ticket `7ef3f8db-d4a9-4135-99eb-3c006070a328` as an appended `## Acceptance Criteria` section:

1. **Target files** — the definitive list of files the implementation will create or modify, as repo-root-relative forward-slash paths, each with a one-line reason.
2. **Numbered acceptance criteria** — each independently verifiable and phrased as an observable behavior, not an activity. A criterion a reviewer cannot mark pass/fail from evidence alone is a defect. "Inheritance works correctly" is wrong; "loading a schema whose declared parent does not exist fails the whole load and leaves the prior registry generation intact" is right.
3. **Validation commands** — the exact commands that prove those criteria, with real verified package names. An invented package name is a defect; if a name is uncertain, mark it explicitly as unverified rather than guessing.
4. **Explicit non-goals** — what the implementation ticket must NOT do, so scope creep is visible at review time.

Rationale: implementation tickets in this program were found to lack target files, test seams, and validation commands. Those fields are the deliverable of planning, not of implementation — an implementation ticket cannot name its own test seams before its research and planning predecessors have run. Writing them back into the implementation ticket at planning time is what makes the plan artifact's consumer contract concrete.
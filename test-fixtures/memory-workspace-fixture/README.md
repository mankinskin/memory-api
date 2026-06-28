# Memory Workspace Fixture

This fixture models a multi-store, multi-worktree layout for E2E and benchmark tests.

The `submodule-a` and `submodule-b` directories emulate submodule worktrees in a deterministic fixture tree.
The `fixtures.toml` manifest is the source of truth used by the `memory-fixtures` crate.

## Layout

```
memory-workspace-fixture/
├── fixtures.toml            # manifest: worktrees + store inventory
├── .ticket/ .spec/          # root-level checked-in stores (seeded)
├── .rule/ .session/ .test-domain/ .log/  # generated representative stores
├── docs/ src/               # generated doc and audit inputs
├── submodule-a/.ticket/     # emulated submodule worktree A
└── submodule-b/.spec/       # emulated submodule worktree B
```

## Consuming the fixture

Use the `memory-fixtures` crate from a test or benchmark:

```rust
use memory_fixtures::materialize_fixture;

let fixture = materialize_fixture().unwrap();          // copies into a tempdir
let ticket_root = fixture.store_root("ticket-root");   // resolved per-domain path
```

`materialize_fixture` copies the fixture into an isolated tempdir so tests can mutate it freely.
During materialization, the loader deterministically adds representative root-domain data:
generated tickets with state/history variation, a searchable rule, a linked session transcript,
a validation execution, a log capture, and doc/audit input files. These generated seeds keep the
checked-in fixture small while giving matrix and benchmark consumers realistic cross-store data.
`materialize_fixture_with_generated_tickets(n)` additionally seeds `n` generated tickets in the
root ticket store for the benchmark-scale variant.

## Adding a store type or scenario

1. Add the seeded entity files under the appropriate worktree directory
   (for example `.rule/rules/<id>/` or `submodule-a/.spec/specs/<uuid>/spec.toml`).
2. Register the store in `fixtures.toml` under a new `[[stores]]` entry with a unique
   `domain` and the `relative_path` to its hidden store directory.
3. Reference the new domain from tests via `fixture.store_root("<domain>")`.

If the store data should be generated rather than committed, add the deterministic seeding logic
to `crates/memory-fixtures/src/lib.rs` and register the generated store path in `fixtures.toml`.

## Regenerating the large benchmark variant

The large variant is generated at load time (no checked-in bulk data):

```rust
let fixture = memory_fixtures::materialize_fixture_with_generated_tickets(200).unwrap();
```

Adjust the count in `crates/ticket-api/benches/fixture_scan.rs` to change benchmark scale.

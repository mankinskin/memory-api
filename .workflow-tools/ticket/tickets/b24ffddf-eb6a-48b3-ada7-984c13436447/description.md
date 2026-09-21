## Problem

`memory-api/crates/peek-api/src/lib.rs` line 544 hardcodes the description `"ticket-cli (state machine, board, deps)"`. The stale source text propagates into generated `repo_map.toon` line 295. `repo_map.toon` lines 72-136 also still describe the deleted `memory-api/crates/memory-api` crate.

The map is inaccurate because the old `ticket-cli` package was replaced by the public `ticket` crate, and the shared base moved to `memory-kernel`. A generated repository map that advertises deleted components misroutes repository exploration.

## Required State

Update the source description to use the current `ticket` terminology, then regenerate `repo_map.toon` with `cargo run -p peek-cli -- . --repo-map --output repo_map.toon`. Run the regeneration after ticket A2 is complete so the regenerated map includes corrected tooling metadata.

Related migration tickets: `ba4aaa9c`, `1b7e0c3d`, and `0da6894c`.

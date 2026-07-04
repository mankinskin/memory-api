Additional evidence from the second perf slice:

- Sequential heavy moves (heavier tracked-reference fixture) took about 27.8 s and 28.0 s.
- Per-phase journal timing shows the dominant execute cost is target rescan, with `scan_target_ms` about 22.3 s and `scan_source_ms` about 5.3 s. In the same runs, `rename_entity_ms`, `rewrite_path_refs_ms`, and `validate_move_ms` were effectively negligible.
- A dedicated `open_or_init_root_perf_fixture` Criterion run projected about 376 s for 10 samples, which strongly suggests store bootstrap / open-or-init is itself extremely expensive on representative fixture sizes.

This narrows the likely bottlenecks further:
1. target-store rescans during move execution,
2. source-store rescans during move execution,
3. store bootstrap / open_or_init work,
4. full scan(true) work.

The pure health finding collection path remains comparatively small relative to the full e2e wall time.
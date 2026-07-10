Tracker for fixing two `rule sync-targets` defects.

Spec: rule-cli/sync-targets/incremental-and-normalized-paths (9c7c0655-436c-4cd8-a3d3-2d893f1d865c)

Problems:
1. Slow/inefficient: full work every run (non-incremental store scan, double rule collection, unconditional writes, per-target SpecStore reopen).
2. Mixed path separators in output (raw PathBuf backslashes vs normalized forward slashes).

Children:
- Normalize path separators in sync-targets output.
- Skip-unchanged writes + single collection pass + spec-store reuse.
- Incremental rule store scan (mtime/hash short-circuit).

Done when all children are done and spec acceptance criteria pass with before/after timing captured.
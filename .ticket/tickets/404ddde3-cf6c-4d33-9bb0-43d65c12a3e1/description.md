## Problem
A generated move journal mixes Windows and Unix separators in persisted path fields and stores extremely large `previous_content` snapshots for rewritten tracked files.

## Required behavior
- Persist journal paths in a stable normalized form.
- Avoid embedding full file contents in `rewritten_path_files` when rollback can restore tracked files from git.
- Keep rollback reliable for tracked rewrites and retain explicit manual follow-ups for cases that cannot be restored automatically.

## Validation
- Focused move-kernel tests for journal persistence and rollback metadata.
- Re-check the generated journal shape after a representative move execution path.
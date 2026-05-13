# File Move Detection

## Objective

Detect when implementation files are moved/renamed and automatically update spec code references.

## Approach

1. **Watcher-based**: Use filesystem watcher to detect rename events
2. **Scan-based**: Periodically compare stored file paths against workspace, finding moved files by content hash
3. **CLI trigger**: `spec sync --check-moves` to scan for moved files

## Acceptance Criteria

- [ ] Detect renamed files via content hash matching
- [ ] Update code_refs with new file paths
- [ ] Report moves in `spec sync` output
- [ ] Handle file deletions (mark refs as broken)
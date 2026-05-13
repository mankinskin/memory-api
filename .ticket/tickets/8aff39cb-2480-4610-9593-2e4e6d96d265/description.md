# [Board][Design] Draftboard Data Model, API Contract, and CLI/MCP Surface

## Objective

Produce the implementation-ready contract for the draftboard system: data model, store API, CLI subcommand surface, and MCP tool definitions. This design must be approved before any implementation begins.

## Context

The draftboard extends the existing ticket system with a workspace-scoped coordination layer. It builds on the existing `LeaseInfo` / `claim()` / `unclaim()` infrastructure in `ticket-api` but adds:

- **Board entries** with file ownership tracking
- **WIP limits** (workspace-configurable maximum concurrent entries)
- **Stale detection** with TTL-based heartbeat expiry
- **File conflict detection** across concurrent entries
- **Lock-gated snapshot** for new agent session onboarding
- **Human escalation** flags for conflicts that cannot be auto-resolved

## Data Model Design

### Board Entry

```rust
/// A draftboard entry representing one agent's active work on one ticket.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BoardEntry {
    /// Immutable unique identifier for this entry (UUID). Survives completion and is
    /// used in confirmation tokens, audit logs, and attempt linkage.
    entry_id: Uuid,
    /// The ticket being worked on.
    ticket_id: Uuid,
    /// Identity of the agent session (e.g. "copilot-session-abc123").
    agent_id: String,
    /// Link to a prior completed entry for the same (ticket_id, agent_id) pair.
    /// Populated when an agent re-checks into a ticket after a previous attempt.
    previous_attempt: Option<Uuid>,
    /// When the agent checked in.
    checked_in_at: DateTime<Utc>,
    /// Last heartbeat timestamp. Updated by `board heartbeat`.
    last_heartbeat: DateTime<Utc>,
    /// Heartbeat TTL in seconds. Entry becomes stale after last_heartbeat + ttl_secs.
    ttl_secs: u64,
    /// Short description of what the agent intends to do.
    intent: String,
    /// Files the agent declares ownership of (workspace-relative paths, lexically normalized).
    owned_files: Vec<String>,
    /// Current entry status, computed from heartbeat freshness and conflict state.
    status: BoardEntryStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum BoardEntryStatus {
    /// Agent is actively working; heartbeat is fresh.
    Active,
    /// Heartbeat expired past TTL — agent may have crashed or abandoned work.
    Stale,
    /// File ownership overlap detected with another entry — needs human resolution.
    Conflict,
    /// Agent checked out cleanly; entry retained briefly for audit before pruning.
    Completed,
}
```

### Board Configuration

```rust
/// Workspace-level board settings, stored in a redb config table or workspace TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BoardConfig {
    /// Maximum concurrent active entries. Check-in is rejected when this limit is reached.
    /// Default: 5.
    max_wip: u32,
    /// Seconds after last heartbeat before an entry is marked stale and escalated for human review.
    /// Default: 3600 (1 hour).
    stale_after_secs: u64,
    /// Minimum retention window for completed entries before they are eligible for explicit cleanup.
    /// Default: 3600 (1 hour). Cleanup remains explicit even after this window.
    completed_audit_window_secs: u64,
}
```

### Board Snapshot

```rust
/// Atomic snapshot of the entire board state, returned by `board show`.
struct BoardSnapshot {
    /// Timestamp of this snapshot.
    captured_at: DateTime<Utc>,
    /// All active entries (not completed/pruned).
    entries: Vec<BoardEntry>,
    /// Entries owned by the caller (when `agent_id` was supplied). Separated for
    /// prominent resume recommendations. Empty when no agent_id was requested.
    caller_entries: Vec<BoardEntry>,
    /// Current board configuration.
    config: BoardConfig,
    /// Number of active (non-stale, non-completed) entries.
    active_count: u32,
    /// Number of stale entries (flagged for attention).
    stale_count: u32,
    /// Number of entries in conflict state.
    conflict_count: u32,
    /// Whether WIP limit is reached (active_count >= max_wip).
    wip_limit_reached: bool,
    /// File ownership map: file path → agent_id(s) owning it. Entries with >1 owner are conflicts.
    file_ownership: BTreeMap<String, Vec<String>>,
    /// Warnings for the onboarding agent session.
    warnings: Vec<String>,
}

/// Transport-layer result for `ticket board show` in CLI/MCP surfaces.
/// The snapshot itself is read-only; `heartbeat` is populated only when the client
/// requested auto-heartbeat and the transport performed a follow-up `board_heartbeat()`.
struct BoardShowResult {
    snapshot: BoardSnapshot,
    heartbeat: Option<BoardEntry>,
}
```

### Storage

- **New redb table `BOARD_ENTRIES`**: Key = `entry_id` (UUID string), Value = bincode-serialized `BoardEntry`.
- **Secondary index `BOARD_ACTIVE_INDEX`**: Key = `"{ticket_id}:{agent_id}"` → `entry_id`. Enforces one active entry per `(ticket_id, agent_id)` pair. Completed/cleaned entries are removed from this index but remain in `BOARD_ENTRIES` until explicitly cleaned.
- **New redb table `BOARD_CONFIG`**: Key = `"default"` (single row), Value = bincode-serialized `BoardConfig`.
- Board entries are **ephemeral** — they are not stored on disk as TOML files. They live only in the redb index.
- On check-in, the board also calls `store.claim()` internally for backward compatibility with existing lease consumers.

## API Surface (ticket-api)

New methods on `TicketStore`:

```rust
impl TicketStore {
    /// Check in: register active work. Returns error if WIP limit reached or file conflict detected.
    pub fn board_check_in(&self, entry: BoardCheckIn) -> Result<BoardEntry, StorageError>;

    /// Check out: mark work completed and release file ownership.
    /// `handoff_reason` captures why the agent is releasing (e.g. session end, handoff to another agent).
    pub fn board_check_out(&self, ticket_id: &Uuid, agent_id: &str, handoff_reason: Option<&str>) -> Result<(), StorageError>;

    /// Heartbeat: renew TTL for an existing entry.
    pub fn board_heartbeat(&self, ticket_id: &Uuid, agent_id: &str) -> Result<BoardEntry, StorageError>;

    /// Show: lock-gated atomic snapshot of the entire board.
    /// This method is read-only and never mutates stale status, cleanup state, or heartbeats.
    pub fn board_show(&self) -> Result<BoardSnapshot, StorageError>;

    /// Configure: update board settings.
    pub fn board_configure(&self, config: BoardConfig) -> Result<BoardConfig, StorageError>;

    /// Clean (preview): return candidates eligible for cleanup and a confirmation token.
    pub fn board_clean_preview(&self) -> Result<BoardCleanPreview, StorageError>;

    /// Clean (apply): remove entries identified in a previously generated preview.
    /// Rejects the token if the board has changed materially since the preview was generated.
    pub fn board_clean_apply(&self, token: &str, include_stale: bool) -> Result<BoardCleanResult, StorageError>;

    /// Update file ownership for an existing entry (add/remove files mid-session).
    /// When `add` and `remove` target the same logical file (old path removed, new path added),
    /// the operation is treated as an atomic rename transition for audit purposes.
    pub fn board_update_files(&self, ticket_id: &Uuid, agent_id: &str, add: &[String], remove: &[String]) -> Result<BoardEntry, StorageError>;

    /// Rename a file in an existing entry's ownership set. Atomically releases the old
    /// path and claims the new one, recorded as a single rename audit event.
    pub fn board_rename_file(&self, ticket_id: &Uuid, agent_id: &str, old_path: &str, new_path: &str) -> Result<BoardEntry, StorageError>;
}
```

### Check-in Validation Rules

1. **WIP limit**: Count active entries. If `active_count >= max_wip`, reject with `BoardError::WipLimitReached { current, max }`.
2. **File conflict**: Scan all active entries' `owned_files`. If any overlap with the new entry's files, reject with `BoardError::FileConflict { files, conflicting_agent }`. Mark both entries as `Conflict` status.
3. **Duplicate check-in**: If the same `(ticket_id, agent_id)` pair already has an active entry, reject with `BoardError::AlreadyCheckedIn`.
4. **Ticket existence**: Verify ticket exists in the store.
5. **Lease propagation**: On successful check-in, call `store.claim()` to create a backward-compatible lease.

### Error Types

```rust
enum BoardError {
    WipLimitReached { current: u32, max: u32 },
    FileConflict { files: Vec<String>, conflicting_agent: String, conflicting_ticket: Uuid },
    AlreadyCheckedIn { ticket_id: Uuid, agent_id: String },
    NotCheckedIn { ticket_id: Uuid, agent_id: String },
    TicketNotFound(Uuid),
}
```

## CLI Surface (ticket-cli)

New `board` subcommand with sub-subcommands:

```
ticket board show [--agent <AGENT_ID>] [--json]
    Lock-gated atomic snapshot. Returns BoardShowResult.
    Default output: human-readable table + warnings.
    When `--agent` is supplied, the command performs a read-only snapshot first,
    then issues a follow-up heartbeat for that agent.

ticket board check-in <TICKET_ID> --agent <AGENT_ID> [--intent "..."] [--files f1 f2 ...] [--ttl-secs N] [--json]
    Register active work. Validates WIP limit and file conflicts.

ticket board check-out <TICKET_ID> [--agent <AGENT_ID>] [--reason "..."] [--json]
    Deregister from the board. Releases file ownership.
    --reason captures an optional handoff/exit reason for audit.

ticket board heartbeat <TICKET_ID> --agent <AGENT_ID> [--json]
    Renew TTL. Returns updated entry with new expiry.

ticket board configure [--max-wip N] [--stale-after-secs N] [--completed-audit-window-secs N] [--json]
    View (no args) or update board configuration.

ticket board clean preview [--json]
    Preview cleanup candidates and receive a confirmation token.
    Shows which completed (past audit window) and stale entries would be removed.

ticket board clean apply <TOKEN> [--include-stale] [--json]
    Execute cleanup using a previously generated confirmation token.
    Rejects the token if the board has changed materially since the preview.
    With --include-stale, also removes stale entries after human review.

ticket board update-files <TICKET_ID> --agent <AGENT_ID> [--add f1 f2] [--remove f3 f4] [--json]
    Modify file ownership for an existing entry.

ticket board rename-file <TICKET_ID> --agent <AGENT_ID> --from <OLD_PATH> --to <NEW_PATH> [--json]
    Atomic file rename: releases old path and claims new path as a single audited transition.

ticket update <TICKET_ID> ... --board-check-in --agent <AGENT_ID> [--board-intent "..."] [--board-files f1 f2 ...] [--board-ttl-secs N]
    Explicit convenience path. The board check-in is performed only when the caller
    opts in and supplies the required board arguments. There is no automatic check-in
    on every state transition.
```

## MCP Surface (ticket-mcp)

New MCP tools:

| Tool | Parameters | Returns |
|---|---|---|
| `board_show` | `workspace`, `agent_id?` | `BoardShowResult` |
| `board_check_in` | `workspace`, `ticket_id`, `agent_id`, `intent?`, `files[]?`, `ttl_secs?` | `BoardEntry` |
| `board_check_out` | `workspace`, `ticket_id`, `agent_id?`, `reason?` | success/error |
| `board_heartbeat` | `workspace`, `ticket_id`, `agent_id` | `BoardEntry` |
| `board_configure` | `workspace`, `max_wip?`, `stale_after_secs?` | `BoardConfig` |
| `board_clean_preview` | `workspace` | `BoardCleanPreview` |
| `board_clean_apply` | `workspace`, `token`, `include_stale?` | `BoardCleanResult` |
| `board_update_files` | `workspace`, `ticket_id`, `agent_id`, `add[]?`, `remove[]?` | `BoardEntry` |
| `board_rename_file` | `workspace`, `ticket_id`, `agent_id`, `old_path`, `new_path` | `BoardEntry` |

## Resolved Decisions (2026-04-09)

1. **`board_show()` stays read-only.** Stale status is computed in memory during snapshot generation. Cleanup is always a separate explicit step via `board clean`.
2. **Yes, auto-heartbeat on show** when the caller supplies an agent identity, but implemented as a second `board_heartbeat()` call at the CLI/MCP layer after the read-only snapshot.
3. **No automatic check-in on `ticket update`.** Instead, add an explicit `--board-check-in` option on `ticket update` when the caller provides the required board arguments. A future iteration may reverse the coupling and update ticket state from board check-in.
4. **Keep an audit window.** Completed entries remain visible for a configurable audit window and agents should seek user permission before invoking cleanup.
5. **Default TTL is one hour.** After one hour without heartbeat, entries are flagged as stale and surfaced as high-priority human-review items to either renew or clean explicitly.

## Remaining Refinement Blockers

Two follow-up design tickets were created after a fresh review of the full track:

- `84ceb9ce` — closes the open questions around board entry identity, resume semantics, same-agent re-check-in, lease/ticket/board synchronization, and file-path canonicalization.
- `c3143e3c` — defines the human approval and conflict-resolution workflow for stale cleanup, renewal, override, and transfer operations.

A validation ticket was also created:

- `be38e809` — validates concurrency, crash recovery, and cross-interface consistency.

**All three refinement tickets are now resolved** (2026-04-12). The full interview record is in `agents/interviews/20260409_INTERVIEW_DRAFTBOARD_REFINEMENT.md`. Key decisions incorporated by reference:

### Identity, Resume, and Synchronization (from `84ceb9ce`)
- Hybrid `entry_id` + active uniqueness on `(ticket_id, agent_id)`.
- Re-check-in creates a new linked attempt; completed entries are never overwritten.
- One active entry per ticket in v1.
- Resume strongly recommended before new work; caller-owned section in `board show`.
- Handoff = check-out + new check-in with optional reason metadata.
- Board is canonical ownership; leases are internal mirrors; ticket state is advisory.
- Public `claim`/`unclaim` removed in favor of board commands.
- All mutating lifecycle operations trigger board reconciliation.
- Workspace-relative lexical path normalization; platform-aware case sensitivity.
- Files + explicit renamed-file transitions in v1.

### Stale Cleanup and Conflict Resolution (from `c3143e3c`)
- Cleanup uses preview-generated confirmation tokens (CLI and MCP aligned).
- One-hour threshold marks entries review-required stale, not auto-cleanable.
- v1 conflict actions: renew, release specific files, mark abandoned + clean.
- Conservative v1 policy: block aggressively, require human review for ambiguity.
- Audit records go to board state + ticket history.

### Validation Bar (from `be38e809`)
- All four race conditions (overlapping-file, WIP-limit, show/heartbeat, clean/checkout) require automated tests.
- Restart recovery: persisted entries loaded, stale recomputed from current time.
- CLI/MCP consistency: same semantics and same core fields; structural field parity test harness.

## Alternatives Considered

### A: Extend LeaseInfo directly
- Pros: No new tables; reuses existing infrastructure
- Cons: LeaseInfo keyed by ticket_id only (one holder per ticket); no file ownership; would break existing consumers

### B: File-based board (TOML on disk)
- Pros: Visible in git; easy to inspect
- Cons: No atomic snapshots; file locking is fragile; cross-platform issues

### C: Separate redb tables (chosen)
- Pros: Atomic transactions; compound keys allow multiple agents per ticket; clean separation from core ticket data
- Cons: Board state not visible on disk; lost on redb rebuild (acceptable — board is ephemeral)

## Deliverables

- [x] Finalized `BoardEntry`, `BoardConfig`, `BoardSnapshot` structs
- [x] Finalized `BoardShowResult` transport wrapper for CLI/MCP `board show`
- [x] API contract approved (method signatures, validation rules, error types)
- [x] CLI subcommand arguments and output format approved
- [x] MCP tool schema approved
- [x] Storage design approved (redb tables, key scheme, serialization)
- [x] Question resolution documented

## Acceptance Criteria

- [x] Data model is concrete, typed, and implementation-ready
- [x] API contract specifies all validation rules and error cases
- [x] CLI and MCP surfaces are fully specified with argument types and return shapes
- [x] Storage design handles concurrent access safely (redb transactions)
- [x] Read-only snapshot behavior, explicit cleanup flow, update-command check-in option, audit window, and one-hour TTL are documented
- [x] Architecture ticket updated to reference the draftboard design

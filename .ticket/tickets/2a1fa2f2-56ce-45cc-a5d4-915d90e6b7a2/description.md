# Phase 1.5 — Lease Protocol

**Status:** BLOCKED (requires Phase 1 CRUD stable)

Global progress tracking: `../EXECUTION_CHECKLIST.md`.
Checkboxes in this file are phase-scope deliverable gates.

## Objective

Specify and implement the lease-based ownership protocol for agent coordination.
Leases are the mechanism by which agents claim exclusive work ownership over tickets
for extended periods, distinct from the short-lived per-ticket write locks.

## Lease vs Lock Distinction

| Concern | Per-ticket lock | Lease |
|---------|----------------|-------|
| Purpose | Protect atomic FS + index writes | Declare work ownership |
| Duration | Milliseconds (single write op) | Minutes to hours |
| Holder | Any writer (CLI, HTTP, watcher) | Worker process (agent) |
| Failure mode | Lock file + timeout | Heartbeat expiry |
| Granularity | One ticket | One ticket |

## Lease State Machine

```
unclaimed ──claim──► claimed ──heartbeat──► claimed (renewed)
    ▲                   │                       │
    │                   │                       │
    │              ┌────┘                       │
    │              ▼                            ▼
    │          expired ◄──────── (TTL elapsed without heartbeat)
    │              │
    └──reclaim─────┘
    │
    └──release─────── claimed ──unclaim──► unclaimed
```

States:
- `unclaimed`: no active lease; ticket is available for work
- `claimed`: lease held by a worker; heartbeat required to maintain
- `expired`: heartbeat missed beyond TTL; eligible for reclaim by any worker

## Heartbeat Protocol

- **Owner:** the worker process (not the CLI shell that invoked `ticket claim`)
- **Interval:** 30 seconds (configurable)
- **TTL:** 120 seconds from last heartbeat (configurable, must be > 2× interval)
- **Mechanism:** manual `ticket heartbeat` for stateless transports, or automatic renewal by `ticket serve --stdio` while the session remains alive
- **Grace period:** on first claim, TTL starts from claim timestamp

## Session-Liveness Renewal

`ticket serve --stdio` is the preferred lease transport for long-running agents.

- the session binds a worker identity and explicit `index_root` at startup
- claims issued by that session are renewed automatically while the stdio connection remains healthy
- explicit `heartbeat` remains available for `ticket exec`, HTTP, MCP, and recovery tooling
- if the session drops, auto-renewal stops and normal TTL expiry applies

Tradeoff accepted: a live but stalled worker can retain a lease until the connection closes or the session is explicitly terminated.

## Claim Collision Rules

1. If ticket is `unclaimed` → grant lease, record `working_by`, `lease_expires_at`, `work_intent`
2. If ticket is `claimed` with valid (non-expired) lease → reject with `LeaseConflict` error
   containing current holder info
3. If ticket is `claimed` but lease has expired → allow reclaim (treat as `expired` → `claimed`),
   log stale lease cleanup event
4. Claim requires per-ticket lock for the lease mutation itself (lock is released immediately
   after lease row write, not held for the duration of the lease)

## Stale Lease Recovery

- Background watchdog (part of `ticket watch` or on-demand via `ticket scan`) scans
  `LEASES` table for entries where `lease_expires_at < now()`
- Expired leases are cleared: row removed, ticket state reverted to `unclaimed` equivalent
- Stale recovery emits a structured diagnostic event with ticket ID, previous holder, and
  expiry timestamp

## Conflict Domains

A conflict domain is an optional string tag on a ticket that declares a mutual-exclusion
group. Two tickets with the same `conflict_domain` value cannot be simultaneously leased
by different workers.

- Conflict domain is a regular field in the ticket manifest (e.g. `conflict_domain: "auth-module"`)
- On `claim`, the system checks active leases for any ticket with the same conflict domain
- If a conflict-domain collision is found, claim is rejected with `ConflictDomainCollision`
  error listing the conflicting ticket and holder

## Validation Assignment Constraints

- Coordinator assigns validator agents when ticket enters `validating`.
- Separation of duties: worker and validator must not be the same identity.
- Validator claims use normal lease protocol (`task_claim`) and are visible in lease views.
- Validation rejection returns ticket to `review` with evidence references and optional linked bug IDs.

## Deliverables

- [ ] `ticket claim <id> [--intent <text>]` — acquire lease
- [ ] `ticket unclaim <id> [--reason <text>]` — release lease
- [ ] `ticket heartbeat <id>` — renew lease TTL
- [ ] `ticket leases` — list all active leases with expiry info
- [ ] `ticket serve --stdio` — persistent JSONL session with request IDs
- [ ] Session-bound automatic lease renewal for claims owned by a live stdio session
- [ ] Stale lease watchdog in `ticket watch` background process
- [ ] Conflict domain check on claim path
- [ ] Policy check: reject `validating` assignment when validator identity equals worker identity
- [ ] Lease state visible in `ticket get <id>` output

## Protocol Examples

```bash
# Claim a ticket for work
ticket claim a3f2c7b1-... --intent "implementing login page" --json

# Heartbeat (called periodically by worker process)
ticket heartbeat a3f2c7b1-... --json

# Release when done
ticket unclaim a3f2c7b1-... --reason "implementation complete" --json

# List active leases
ticket leases --json
```

```json
{"id":1,"command":"task_claim","id":"a3f2c7b1-4e9d-4f0a-8c3b-1d2e5f6a7b8c","intent":"implementing login page"}
{"id":2,"command":"task_unclaim","id":"a3f2c7b1-4e9d-4f0a-8c3b-1d2e5f6a7b8c","reason":"implementation complete"}
```

## Risks

- Heartbeat failure during network partition leaves tickets locked until TTL expires.
- Clock skew between agents can cause premature or delayed expiry; use monotonic time
  where possible and document expected clock accuracy requirements.
- Conflict domain collision logic scales linearly with number of active leases; acceptable
  at <1000 concurrent leases, revisit if scale grows.

## TODO

- TODO: Define lease event log format for observability / Phase 5 messenger integration.
- TODO: Decide whether heartbeat interval and TTL are per-ticket or global settings.
- TODO: Write integration test: claim → heartbeat loop → expire → reclaim by different worker.
- TODO: Define how `ticket serve --stdio` reports session-owned lease renewals in event logs and diagnostics.

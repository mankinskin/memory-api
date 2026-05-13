# [bootstrap][T1] Startup and Auth Bootstrap for Host Executor

## Context

The host executor is a Rust service process (`ticket host-executor`) that workers authenticate against to claim tickets, run inference, and report progress. Per the Phase 1.5 design, the executor can run in `stdio` mode (local foreground) or `http` mode (local daemon). Workers receive short-lived HMAC-signed executor tokens — never raw provider API keys.

This ticket covers the first integration test scenario: the executor starts correctly, mints an executor token, and rejects unauthorized requests.

## Scope

### What is being built / tested
- Host executor binary startup in `stdio` mode with a test provider adapter (no real inference calls)
- Token minting: coordinator mints a scoped HMAC executor token for a worker assignment
- Token verification: executor validates token signature + expiry on each request
- Unauthorized request rejection: verifies deny reasons are structured and machine-readable

### Not in scope
- HTTP mode server hardening (Phase 5)
- Real provider adapter integration (covered by `8c185de3`)
- Branch/cwd enforcement (T2)
- Ticket mutation operations (T3)

## Token Contract (Phase 1.5)

Executor token minimum claims:
- `sub` — worker identity
- `assignment_id`
- `ticket_id`
- `scope` — allowed actions (`claim`, `update`, `unclaim`, `inference`)
- `exp` — short expiry (≤ 15 minutes)
- `nonce` — replay protection (UUIDv4)

Deny reasons must be structured:
- `auth.invalid_signature`
- `auth.expired_token`
- `auth.assignment_not_active`
- `auth.scope_denied`
- `auth.ticket_mismatch`

## Open Decisions to Resolve Before Implementation

- **Token format**: JWT (standard library support, widely understood) vs compact HMAC envelope (lighter, custom). Decision drives the signing/verification implementation.
- **Default provider for local dev**: which `TICKET_LLM_PROVIDER` value should the test harness use? Recommend `local` (no network) with a stub adapter.
- **Inference scope split**: is inference a separate scope from ticket mutation (`claim`, `update`)? Recommend yes — principle of least privilege.

## Acceptance Criteria

- [ ] `ticket host-executor --listen stdio --provider local` starts without error in a test harness
- [ ] Coordinator mints a valid executor token for a worker assignment
- [ ] Worker authenticates successfully with a valid in-scope token
- [ ] Request with an expired token is rejected with `auth.expired_token`
- [ ] Request with an invalid signature is rejected with `auth.invalid_signature`
- [ ] Request for an out-of-scope action is rejected with `auth.scope_denied`
- [ ] All deny responses are structured (no plain-text error strings)
- [ ] Token signing key is loaded from environment only — not hardcoded

## Dependencies

- Depends on: none (first test in the sequence)
- Blocks: T2, T3, T4, T5, T6 (sequential integration suite)
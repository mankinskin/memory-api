# [AOH][Research] Sandbox Isolation Technologies for Agent Code Execution

## Status: COMPLETE — All decisions locked 2026-04-13

> **ADR-1 v1 Selection: Tier 2 — Container (Docker / Podman via `bollard`)**
>
> Per the locked architecture decision (ADR-1 in `34bc4938`), v1 uses **container-based isolation** with Docker (primary) and Podman (Linux CI) orchestrated through the `bollard` crate. MicroVMs (Firecracker), cloud sandboxes (E2B), and devcontainer managers (Daytona) are **background research only** — they are not in scope for v1 implementation.
>
> Readers working on implementation tickets should focus on Tier 2. Other tiers remain documented here for future reference.

## Resolved Decisions (2026-04-13)

| Decision | Resolution |
|---|---|
| **Development default (Windows)** | Tier 2 — Docker Desktop on WSL2, consistent with CI/prod. Tier 0 (git worktree only) as fallback only when Docker is unavailable. |
| **Secret injection** | One-time HTTP/Unix-socket fetch with nonce + TTL (ADR-13). Cross-reference `db784443` for full secret lifecycle. |
| **Cold-start budget** | Target <5 seconds from `docker run` to agent prompt for pre-pulled images. Container BaaS ticket (`49d6fe2e`) targets <3s optimistically; 5s is the hard ceiling. |

## Objective

Identify the right sandbox isolation layer for running agent sessions: from lightweight git worktrees to full VM isolation. Document tradeoffs in security, cold-start latency, resource cost, and implementation complexity within a Rust orchestrator.

## Research Questions

1. What is the startup latency and per-session overhead at each isolation tier?
2. Which options provide filesystem, network, and process isolation independently?
3. Which have safe secret injection (env vars, mounted secrets) without leaking to logs?
4. Can sessions be snapshotted and restored for revival?
5. What Rust APIs or crates exist for each approach?
6. Which approaches work on a Windows dev machine (current OS) vs Linux CI?

## Isolation Tiers to Research

### Tier 0: Git Worktree + Shell Restriction
- `git worktree add` per session
- Restricted PATH; no network by default
- **Already partially designed in T2 (b1f3e2a4)**
- Gaps: no filesystem isolation, no network block, process can escape

### Tier 1: OS-level Namespaces (Linux only)
- `clone(CLONE_NEWNS | CLONE_NEWNET | CLONE_NEWPID)` via `nix` crate
- Unshare network, PID, mount namespaces
- Pivot root / chroot for filesystem confinement
- **Windows incompatible** — needs WSL2 or Linux host

### Tier 2: Container (Docker / Podman / containerd)
- Docker Engine API: `bollard` crate (Rust Docker API client)
- Per-session container with a pre-built agent image
- Volumes for: git worktree (bind mount), secrets (Docker secrets), tool access
- Network: host-network for git ops OR isolated network + proxy
- Snapshot: `docker commit` or overlay FS checkpoint
- **Research**: bollard API completeness, Windows named pipe support

### Tier 3: Firecracker MicroVMs
- https://github.com/firecracker-microvm/firecracker
- Rust binary; fast boot (~125ms); kernel-level isolation
- Jailer for additional restriction; vsock for host-VM communication
- **Research**: Rust HTTP API client for Firecracker API server, snapshot/restore support, Windows compatibility (probably Linux-only)

### Tier 4: E2B Cloud Sandboxes
- https://e2b.dev — Firecracker managed by E2B cloud
- SDK: check for Rust SDK or use HTTP API directly
- Pros: no local infra; built for AI agents; file upload, process execution, networking
- Cons: internet dependency, cost per sandbox-second, data residency

### Tier 5: Daytona / Dev Containers
- https://github.com/daytonaio/daytona
- Git-aware workspace manager; devcontainer.json support
- REST API for workspace lifecycle
- Check: Rust HTTP client integration, snapshot, cost

### Cross-Cutting Concerns

#### Secret Injection
- How do secrets reach the agent without appearing in CLI args or logs?
- Options: env vars, tmpfs-mounted files, in-memory API (vault-style)

#### Network Policy
- Agent needs: git remote access, npm/cargo registry, AI API endpoint
- Agent should NOT have: arbitrary internet access, access to host network services
- Options: allow-list proxy (e.g. `mitmproxy` in mode), netfilter rules, container networks

#### Windows Compatibility
- Current developer OS is Windows
- Tier 0 works on Windows (git worktree)
- Tiers 1–3 require Linux (WSL2 acceptable?)
- E2B/Daytona work from Windows (cloud-managed)

#### Cold-Start Budget
- Acceptable cold-start latency: ? (ask in interview Q1)
- Estimate per tier: Tier 0 (<50ms), Tier 1 (<100ms), Tier 2 (1-10s), Tier 3 (~125ms), Tier 4 (1-3s network)

## Evaluation Matrix

| Tier | Cold Start | FS Isolation | Net Isolation | Snapshot | Windows | Rust API |
|---|---|---|---|---|---|---|
| Git worktree | <50ms | No | No | Manual | Yes | git2/gix |
| OS namespaces | <100ms | Yes | Yes | No | No (WSL2) | nix crate |
| Container | 1-10s | Yes | Yes | commit | WSL2 | bollard |
| Firecracker | ~125ms | Yes | Yes | Native | No | HTTP API |
| E2B cloud | 1-3s | Yes | Yes | Native | Yes | HTTP API |
| Daytona | 2-5s | Yes | Optional | Yes | Yes | REST API |

## Deliverable

Recommendation for:
1. **Development default** (Windows-compatible, fast iteration)
2. **CI/staging** (maximum isolation, reliable cleanup)
3. **Production** (cost-optimized, scalable)

## Acceptance Criteria

- [ ] All 6 tiers evaluated with filled matrix rows
- [ ] Windows vs Linux compatibility documented for each
- [ ] Rust crate / API identified for each viable tier
- [ ] Cold-start latency measured or estimated for top candidates
- [ ] Secret injection approach documented per tier
- [ ] Recommendation for dev/CI/production tiers recorded
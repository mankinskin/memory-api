# Problem

The child-workspace ticket-reference rollout is now split into three well-scoped tickets, but there is no parent tracker that captures the full implementation sequence, the shared goal, or the cross-ticket validation expectations.

Without a parent tracker, it is easy to implement one slice in isolation and miss the required ordering:

1. contract design for workspace-aware refs and removal of `/workspace/default`
2. backend ancestor-endpoint visibility and reversible workspace-aware refs
3. frontend migration to workspace-aware refs for list, detail, graph, history, files, assets, and mutations

# Goal

Track the full child-workspace ticket-reference rollout from contract design through backend support and frontend migration so implementation can proceed in the right order and close as one coherent work track.

# Child tickets

1. `700b9763-17f8-436e-ace0-45b88bedd1d7` — define the ticket-http contract for workspace-aware refs and route migration away from `/workspace/default`.
2. `429f6f1d-6429-4601-bfac-b572fdb4dbff` — implement backend ancestor dependency visibility and workspace-aware refs for downstream resolution.
3. `4629b9d9-3bd0-4ef6-82b6-d6e609c16cac` — migrate ticket-viewer to workspace-aware refs and owning-workspace follow-up flows.

# Acceptance Criteria

- The tracker depends on the three child tickets in implementation order.
- The work track explicitly captures the required sequence: design, backend, then frontend.
- The track keeps `/workspace/default` removal, ancestor-owned dependency visibility, and owning-workspace follow-up flows in one coordinated rollout.
- Closure of this tracker implies the three child tickets are closed and the runtime flow is validated end-to-end.

# Notes

- Spec `0b1888f2-7e59-45fb-95d8-1bf14ff7747f` covers the ancestor-workspace dependency behavior that informs the backend slice.
- Ticket `07836f41-7fa5-4e41-8411-1c7cf8aeee75` remains adjacent tooling work for nested-root CLI metadata and recovery, but it is not a prerequisite for this runtime track.

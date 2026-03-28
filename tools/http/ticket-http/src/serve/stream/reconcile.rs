//! Periodic reconcile loop: emits `snapshot.ready` on a heartbeat interval.
//!
//! In v1 the reconcile is a simple heartbeat — it does not compare storage
//! revision watermarks yet (that is deferred until revision tracking is added
//! to `TicketStore`).  Clients that reconnect after a gap should use the
//! snapshot to rebuild their local state.

use std::{sync::Arc, time::Duration};

use crate::serve::stream::emitter::HookEmitter;
use ticket_api::storage::store::TicketStore;

const RECONCILE_INTERVAL: Duration = Duration::from_secs(30);

/// Spawn a background reconcile task for a single workspace.
///
/// The spawned task runs until the process exits; there is no shutdown channel
/// in v1.  The `store` `Arc` is kept alive by the task.
pub fn spawn_reconcile(
    store: Arc<TicketStore>,
    emitter: HookEmitter,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(RECONCILE_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            // Skip the database read when no SSE clients are connected.
            // The heartbeat is only useful when someone is listening; holding
            // the redb exclusive lock while counting tickets and edges would
            // block other processes (e.g. ticket-mcp) from opening the store.
            if !emitter.has_subscribers() {
                tracing::debug!(
                    workspace = %emitter.workspace,
                    "reconcile: no subscribers, skipping db read"
                );
                continue;
            }

            // Count nodes and edges from the store.
            let (node_count, edge_count) = tokio::task::spawn_blocking({
                let store = Arc::clone(&store);
                move || {
                    let nodes = store.list(None, None, None).map(|v| v.len()).unwrap_or(0);
                    let edges = store.list_all_edges().map(|v| v.len()).unwrap_or(0);
                    (nodes, edges)
                }
            })
            .await
            .unwrap_or((0, 0));

            emitter.snapshot_ready(node_count, edge_count);
            tracing::debug!(
                workspace = %emitter.workspace,
                node_count,
                edge_count,
                "reconcile.snapshot_ready"
            );
        }
    })
}

//! `HookEmitter`: called by `TicketStore` after each mutation to fan out SSE events.
//!
//! The emitter is workspace-scoped and holds an `Arc<StreamBroker>` reference.
//! All calls are synchronous and non-blocking — the underlying broadcast send
//! is a ring-buffer push that never blocks writers.

use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use super::{
    broker::StreamBroker,
    event::{
        DiagnosticWarningPayload, EdgePayload, EdgeRecord, SnapshotReadyPayload,
        SseEvent, TicketDeletePayload, TicketSnapshot, TicketUpsertPayload,
    },
};

/// Workspace-scoped event emitter.  One per `TicketStore` instance.
#[derive(Clone)]
pub struct HookEmitter {
    pub workspace: String,
    broker: Arc<StreamBroker>,
}

impl HookEmitter {
    pub fn new(workspace: impl Into<String>, broker: Arc<StreamBroker>) -> Self {
        Self {
            workspace: workspace.into(),
            broker,
        }
    }

    /// Push a raw `SseEvent` to the broker.
    pub fn emit(&self, event: SseEvent) {
        self.broker.emit(&self.workspace, event);
    }

    // ── Convenience constructors ──────────────────────────────────────────────

    pub fn ticket_upsert(
        &self,
        id: Uuid,
        state: Option<String>,
        title: Option<String>,
        updated_at: chrono::DateTime<Utc>,
    ) {
        self.emit(SseEvent::TicketUpsert(TicketUpsertPayload {
            workspace: self.workspace.clone(),
            ts: Utc::now(),
            ticket: TicketSnapshot {
                id,
                state,
                updated_at,
                title,
            },
        }));
    }

    pub fn ticket_delete(&self, id: Uuid) {
        let now = Utc::now();
        self.emit(SseEvent::TicketDelete(TicketDeletePayload {
            workspace: self.workspace.clone(),
            ts: now,
            id,
            deleted_at: now,
        }));
    }

    pub fn edge_upsert(&self, from: Uuid, to: Uuid, kind: String) {
        self.emit(SseEvent::EdgeUpsert(EdgePayload {
            workspace: self.workspace.clone(),
            ts: Utc::now(),
            edge: EdgeRecord { from, to, kind },
        }));
    }

    pub fn edge_delete(&self, from: Uuid, to: Uuid, kind: String) {
        self.emit(SseEvent::EdgeDelete(EdgePayload {
            workspace: self.workspace.clone(),
            ts: Utc::now(),
            edge: EdgeRecord { from, to, kind },
        }));
    }

    /// Returns `true` if at least one SSE subscriber is currently connected
    /// for this workspace.  The reconcile loop uses this to skip expensive
    /// database reads when nobody is listening.
    pub fn has_subscribers(&self) -> bool {
        self.broker.workspace_subscriber_count(&self.workspace) > 0
    }

    pub fn snapshot_ready(&self, node_count: usize, edge_count: usize) {
        self.emit(SseEvent::SnapshotReady(SnapshotReadyPayload {
            workspace: self.workspace.clone(),
            ts: Utc::now(),
            snapshot_id: Uuid::new_v4(),
            node_count,
            edge_count,
        }));
    }

    pub fn diagnostic_warning(&self, code: impl Into<String>, message: impl Into<String>) {
        self.emit(SseEvent::DiagnosticWarning(DiagnosticWarningPayload {
            workspace: self.workspace.clone(),
            ts: Utc::now(),
            code: code.into(),
            message: message.into(),
        }));
    }
}

impl ticket_api::storage::store::StoreHook for HookEmitter {
    fn ticket_upsert(
        &self,
        id: Uuid,
        state: Option<String>,
        title: Option<String>,
        updated_at: chrono::DateTime<Utc>,
    ) {
        self.ticket_upsert(id, state, title, updated_at);
    }

    fn ticket_delete(&self, id: Uuid) {
        self.ticket_delete(id);
    }

    fn edge_upsert(&self, from: Uuid, to: Uuid, kind: String) {
        self.edge_upsert(from, to, kind);
    }

    fn edge_delete(&self, from: Uuid, to: Uuid, kind: String) {
        self.edge_delete(from, to, kind);
    }
}

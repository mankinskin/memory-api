//! SSE event types matching sse-schema-v1.md (frozen in ticket 09a32876).
//!
//! All events share the envelope: `id` (monotonic), `event` (name), `data` (JSON).

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// All possible SSE event payloads for the ticket graph stream.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "_event_type", rename_all = "snake_case")]
pub enum SseEvent {
    TicketUpsert(TicketUpsertPayload),
    TicketDelete(TicketDeletePayload),
    EdgeUpsert(EdgePayload),
    EdgeDelete(EdgePayload),
    TicketConflict(TicketConflictPayload),
    SnapshotReady(SnapshotReadyPayload),
    DiagnosticWarning(DiagnosticWarningPayload),
}

impl SseEvent {
    /// The SSE `event:` field name.
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::TicketUpsert(_) => "ticket.upsert",
            Self::TicketDelete(_) => "ticket.delete",
            Self::EdgeUpsert(_) => "edge.upsert",
            Self::EdgeDelete(_) => "edge.delete",
            Self::TicketConflict(_) => "ticket.conflict",
            Self::SnapshotReady(_) => "snapshot.ready",
            Self::DiagnosticWarning(_) => "diagnostic.warning",
        }
    }

    /// Serialize this event's payload to a JSON string.
    pub fn data_json(&self) -> Result<String, serde_json::Error> {
        match self {
            Self::TicketUpsert(p) => serde_json::to_string(p),
            Self::TicketDelete(p) => serde_json::to_string(p),
            Self::EdgeUpsert(p) => serde_json::to_string(p),
            Self::EdgeDelete(p) => serde_json::to_string(p),
            Self::TicketConflict(p) => serde_json::to_string(p),
            Self::SnapshotReady(p) => serde_json::to_string(p),
            Self::DiagnosticWarning(p) => serde_json::to_string(p),
        }
    }

    /// Build an `axum::response::sse::Event` from this payload.
    pub fn into_sse_event(self, id: u64) -> axum::response::sse::Event {
        let name = self.event_name();
        let data = self.data_json().unwrap_or_else(|_| "{}".to_string());
        axum::response::sse::Event::default()
            .id(id.to_string())
            .event(name)
            .data(data)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketUpsertPayload {
    pub workspace: String,
    pub ts: DateTime<Utc>,
    pub ticket: TicketSnapshot,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketDeletePayload {
    pub workspace: String,
    pub ts: DateTime<Utc>,
    pub id: Uuid,
    pub deleted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EdgePayload {
    pub workspace: String,
    pub ts: DateTime<Utc>,
    pub edge: EdgeRecord,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketConflictPayload {
    pub workspace: String,
    pub ts: DateTime<Utc>,
    pub id: Uuid,
    pub operation: String,
    pub expected_rev: u64,
    pub observed_rev: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotReadyPayload {
    pub workspace: String,
    pub ts: DateTime<Utc>,
    pub snapshot_id: Uuid,
    pub node_count: usize,
    pub edge_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticWarningPayload {
    pub workspace: String,
    pub ts: DateTime<Utc>,
    pub code: String,
    pub message: String,
}

/// Minimal ticket fields carried in SSE events.
#[derive(Debug, Clone, Serialize)]
pub struct TicketSnapshot {
    pub id: Uuid,
    pub state: Option<String>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Edge record (from, to, kind).
#[derive(Debug, Clone, Serialize)]
pub struct EdgeRecord {
    pub from: Uuid,
    pub to: Uuid,
    pub kind: String,
}

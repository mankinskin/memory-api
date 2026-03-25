//! Real SSE stream handler — `HookEmitter → StreamBroker → live fan-out`.
//!
//! `GET /api/stream?workspace=<name>` subscribes to the per-workspace broadcast
//! channel and streams events to the client as Server-Sent Events.

use axum::{
    extract::{Query, State},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
};
use futures_util::stream::{self, BoxStream, StreamExt};
use serde::Deserialize;
use std::convert::Infallible;
use tokio::sync::broadcast::error::RecvError;

use crate::serve::{
    stream::{
        broker::next_event_id,
        event::{SnapshotReadyPayload, SseEvent},
    },
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    pub workspace: String,
}

pub async fn stream_handler(
    State(state): State<AppState>,
    Query(params): Query<StreamQuery>,
) -> impl IntoResponse {
    let workspace = params.workspace.clone();

    // Collect baseline counts; 0,0 if workspace is unknown.
    let combined: BoxStream<'static, Result<Event, Infallible>> =
        if let Some(store) = state.ensure_workspace_runtime(&workspace) {
            let nc = store.list(None, None, None).map(|v| v.len()).unwrap_or(0);
            let ec = store.list_all_edges().map(|v| v.len()).unwrap_or(0);

            // Subscribe before emitting the snapshot so no events are missed.
            let rx = state.broker.subscribe(&workspace);

            // Initial `snapshot.ready` burst so the client knows the baseline.
            let snapshot_event = SseEvent::SnapshotReady(SnapshotReadyPayload {
                workspace: workspace.clone(),
                ts: chrono::Utc::now(),
                snapshot_id: uuid::Uuid::new_v4(),
                node_count: nc,
                edge_count: ec,
            })
            .into_sse_event(next_event_id());

            let initial =
                stream::once(async move { Ok::<Event, Infallible>(snapshot_event) });

            // Convert the broadcast receiver into an async stream via unfold.
            let live = stream::unfold(rx, |mut rx| async move {
                loop {
                    match rx.recv().await {
                        Ok((id, event)) => {
                            return Some((
                                Ok::<Event, Infallible>(event.into_sse_event(id)),
                                rx,
                            ));
                        }
                        Err(RecvError::Lagged(n)) => {
                            tracing::warn!(dropped = n, "SSE receiver lagged; events dropped");
                            continue;
                        }
                        Err(RecvError::Closed) => return None,
                    }
                }
            });

            initial.chain(live).boxed()
        } else {
            // Unknown workspace — emit a single diagnostic then close.
            stream::once(async move {
                Ok::<Event, Infallible>(
                    Event::default()
                        .event("diagnostic.warning")
                        .data(format!(
                            r#"{{"workspace":"{workspace}","code":"UNKNOWN_WORKSPACE","message":"workspace not found"}}"#
                        )),
                )
            })
            .boxed()
        };

    Sse::new(combined).keep_alive(KeepAlive::default())
}

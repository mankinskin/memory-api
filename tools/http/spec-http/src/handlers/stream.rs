//! Stub SSE stream handler for `GET /api/specs/stream`.
//!
//! Returns a valid `text/event-stream` response that keeps the connection open
//! with periodic keep-alive pings.  No data events are emitted until spec-http
//! implements full SSE fan-out (see spec-viewer `sse.rs` for the client side).
//!
//! The stub prevents the browser from logging a 404 console error when the
//! Dioxus SPA opens an `EventSource` connection on startup.

use std::convert::Infallible;
use std::time::Duration;

use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream;

/// GET /api/specs/stream
///
/// Streams `text/event-stream` with keep-alive pings every 30 s.
/// The stream never ends — the EventSource stays connected.
pub async fn spec_stream() -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    // A pending stream never yields items (no events to push yet).
    let s = stream::pending::<Result<Event, Infallible>>();
    Sse::new(s).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("ping"),
    )
}

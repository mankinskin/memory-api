//! Thin HTTP API layer for ticket-api.
//!
//! Exposes the ticket store over a REST/SSE API with bearer-token auth.
//! Can be used as a library (embed the router in another server) or run
//! as a standalone binary.

pub mod serve;

// Re-export the key types callers need to embed the HTTP API.
pub use serve::{AppState, ServeConfig, WorkspaceRegistry, serve as start_server};
pub use serve::routes::build_router;

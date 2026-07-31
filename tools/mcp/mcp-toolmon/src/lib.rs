//! Model-aware MCP middleware library.
//!
//! See [`gate`] for the cost decision core and [`proxy`] for the pure
//! JSON-RPC interception logic. The binary in `main.rs` wires these to stdio.

pub mod gate;
pub mod proxy;

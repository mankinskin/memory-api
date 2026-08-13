#[path = "args/board.rs"]
mod board;
#[path = "args/core.rs"]
mod core;
#[path = "args/operations.rs"]
mod operations;
#[path = "args/workspace.rs"]
mod workspace;

pub use self::{
    board::*,
    core::*,
    operations::*,
    workspace::*,
};

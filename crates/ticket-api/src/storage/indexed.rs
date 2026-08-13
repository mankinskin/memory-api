pub use memory_kernel::storage::indexed::*;
// Backward-compatible alias: downstream code uses IndexedTicket.
pub use memory_kernel::storage::indexed::{
    IndexedEntity as IndexedTicket,
    WorkflowFacts,
};

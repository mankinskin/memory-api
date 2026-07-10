//! Ticket-domain entry points for journaled cross-workspace moves.
//!
//! All execution logic lives in [`memory_api::storage::move_kernel`]; these
//! methods build a [`TicketMoveDomain`] adapter and delegate to the generic
//! kernel, mapping the kernel error back onto [`StorageError`]. The journal and
//! outcome types are re-exported so existing surfaces keep their public paths.

use uuid::Uuid;

use memory_api::storage::move_kernel;

use crate::{
    error::StorageError,
    storage::{
        move_planner::{
            MovePreflightReport,
            TicketMoveDomain,
            from_move_error,
        },
        store::TicketStore,
    },
};

// Re-export the neutral kernel execution types under their established paths.
pub use memory_api::storage::move_kernel::{
    MoveExecutionPhase,
    MoveJournal,
    MoveManualFollowup,
    MoveOutcome as MoveExecutionOutcome,
    MovePathRewrite,
};

impl TicketStore {
    pub fn execute_move_with_journal(
        &self,
        plan: &MovePreflightReport,
    ) -> Result<MoveExecutionOutcome, StorageError> {
        let domain = TicketMoveDomain::new(self);
        move_kernel::execute_move(&domain, plan).map_err(from_move_error)
    }

    pub fn resume_move_with_journal(
        &self,
        journal_id: Uuid,
    ) -> Result<MoveExecutionOutcome, StorageError> {
        let domain = TicketMoveDomain::new(self);
        move_kernel::resume_move(&domain, journal_id).map_err(from_move_error)
    }

    pub fn rollback_move_with_journal(
        &self,
        journal_id: Uuid,
    ) -> Result<MoveExecutionOutcome, StorageError> {
        let domain = TicketMoveDomain::new(self);
        move_kernel::rollback_move(&domain, journal_id).map_err(from_move_error)
    }
}

#[cfg(test)]
#[path = "move_execution_tests.rs"]
mod tests;

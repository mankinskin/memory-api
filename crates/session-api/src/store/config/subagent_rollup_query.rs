use std::collections::HashMap;

use crate::{SubAgentRollup, compute_subagent_rollups};

impl SessionStoreConfig {
    /// Get subagent rollups for a specific workspace session.
    /// Returns a map keyed by run_id with per-sub-agent token and cost rollups.
    pub fn subagent_rollups(
        &self,
        session_id: &str,
    ) -> Result<HashMap<String, SubAgentRollup>, SessionError> {
        // Read the session record
        let record = self.read_session(session_id)?;
        
        // Try to load the runtime context (may not exist for non-runtime sessions)
        let context = match self.read_runtime_context(session_id) {
            Ok(ctx) => Some(ctx),
            Err(SessionError::RuntimeContextNotFound { .. }) => None,
            Err(err) => return Err(err),
        };
        
        // Compute rollups
        let rollups = compute_subagent_rollups(&record, context.as_ref());
        
        Ok(rollups)
    }
}

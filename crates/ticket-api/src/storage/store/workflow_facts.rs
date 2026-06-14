use std::collections::{
    HashSet,
    VecDeque,
};

use chrono::{
    DateTime,
    Utc,
};
use uuid::Uuid;

use crate::{
    error::StorageError,
    storage::indexed::WorkflowFacts,
};

use super::TicketStore;

impl TicketStore {
    pub(super) fn rebuild_workflow_facts(&self) -> Result<(), StorageError> {
        self.index.clear_workflow_facts()?;
        let all_ticket_ids = self
            .normalize_indexed_tickets(self.index.list_tickets()?)
            .into_iter()
            .map(|ticket| ticket.id)
            .collect::<Vec<_>>();
        self.recompute_workflow_facts_for_ids(&all_ticket_ids, None)
    }

    pub(super) fn refresh_workflow_facts_for_roots(
        &self,
        root_ids: &[Uuid],
        progress: bool,
        changed_at: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        let affected_ids = self.affected_workflow_slice(root_ids)?;
        self.recompute_workflow_facts_for_ids(
            &affected_ids.into_iter().collect::<Vec<_>>(),
            progress.then_some(changed_at),
        )
    }

    pub(super) fn state_rank_for_type(
        &self,
        type_id: &str,
        state: Option<&str>,
    ) -> usize {
        let Some(state) = state else {
            return 0;
        };
        self.schema_registry
            .get(type_id)
            .and_then(|schema| schema.states.iter().position(|value| value == state))
            .unwrap_or(0)
    }

    fn affected_workflow_slice(
        &self,
        root_ids: &[Uuid],
    ) -> Result<HashSet<Uuid>, StorageError> {
        let mut queue = VecDeque::from(root_ids.to_vec());
        let mut visited = HashSet::new();
        let mut affected = HashSet::new();

        while let Some(current) = queue.pop_front() {
            if !visited.insert(current) {
                continue;
            }
            if let Some(_ticket) = self.get_indexed(&current)? {
                affected.insert(current);
            }

            for edge in self.index.edges_to(&current)? {
                if edge.kind == "depends_on" {
                    queue.push_back(edge.from);
                }
            }
        }

        Ok(affected)
    }

    fn recompute_workflow_facts_for_ids(
        &self,
        ticket_ids: &[Uuid],
        progress_at: Option<DateTime<Utc>>,
    ) -> Result<(), StorageError> {
        let existing = self.index.get_workflow_facts_many(ticket_ids)?;

        for ticket_id in ticket_ids {
            let Some(ticket) = self.get_indexed(ticket_id)? else {
                self.index.remove_workflow_facts(ticket_id)?;
                continue;
            };

            let dependency_ids = self
                .index
                .edges_from(ticket_id)?
                .into_iter()
                .filter(|edge| edge.kind == "depends_on")
                .map(|edge| edge.to)
                .collect::<Vec<_>>();
            let dependencies = self.get_indexed_many(&dependency_ids)?;
            let unresolved_dependency_count = dependency_ids
                .iter()
                .filter(|dependency_id| {
                    dependencies
                        .get(dependency_id)
                        .map(|dependency| !is_done_state(dependency.state.as_deref()))
                        .unwrap_or(true)
                })
                .count();

            let old_facts = existing.get(ticket_id);
            let became_actionable_at = if unresolved_dependency_count == 0 {
                match old_facts {
                    Some(facts) if facts.unresolved_dependency_count > 0 => {
                        Some(progress_at.unwrap_or(ticket.updated_at))
                    }
                    Some(facts) => facts.became_actionable_at.or(Some(ticket.created_at)),
                    None => Some(ticket.created_at),
                }
            } else {
                None
            };
            let last_blocker_progress_at = if unresolved_dependency_count == 0 {
                None
            } else {
                progress_at.or_else(|| {
                    old_facts.and_then(|facts| facts.last_blocker_progress_at)
                })
            };

            self.index.insert_workflow_facts(
                ticket_id,
                &WorkflowFacts {
                    unresolved_dependency_count,
                    became_actionable_at,
                    last_blocker_progress_at,
                },
            )?;
        }

        Ok(())
    }
}

fn is_done_state(state: Option<&str>) -> bool {
    matches!(state, Some("done" | "cancelled"))
}
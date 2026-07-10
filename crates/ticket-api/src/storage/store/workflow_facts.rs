use std::collections::{
    BTreeMap,
    HashSet,
    VecDeque,
};

use chrono::{
    DateTime,
    Utc,
};
use std::time::Instant;
use uuid::Uuid;

use crate::{
    error::StorageError,
    storage::indexed::WorkflowFacts,
};

use super::TicketStore;

impl TicketStore {
    pub(super) fn rebuild_workflow_facts(
        &self
    ) -> Result<BTreeMap<String, u64>, StorageError> {
        let overall_started = Instant::now();
        let mut timings = BTreeMap::new();
        let clear_started = Instant::now();
        self.index.clear_workflow_facts()?;
        add_timing(
            &mut timings,
            "workflow.clear_existing_facts_ms",
            clear_started,
        );

        let list_started = Instant::now();
        let all_ticket_ids = self
            .normalize_indexed_tickets(self.index.list_tickets()?)
            .into_iter()
            .map(|ticket| ticket.id)
            .collect::<Vec<_>>();

        add_timing(&mut timings, "workflow.list_all_tickets_ms", list_started);

        self.recompute_workflow_facts_for_ids_with_timings(
            &all_ticket_ids,
            None,
            Some(&mut timings),
        )?;

        add_timing(
            &mut timings,
            "workflow.recompute_total_ms",
            overall_started,
        );

        Ok(timings)
    }

    pub(super) fn refresh_workflow_facts_for_roots(
        &self,
        root_ids: &[Uuid],
        progress: bool,
        changed_at: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        self.refresh_workflow_facts_for_roots_with_timings(
            root_ids, progress, changed_at,
        )?;
        Ok(())
    }

    pub(super) fn refresh_workflow_facts_for_roots_with_timings(
        &self,
        root_ids: &[Uuid],
        progress: bool,
        changed_at: DateTime<Utc>,
    ) -> Result<BTreeMap<String, u64>, StorageError> {
        let mut timings = BTreeMap::new();
        let affected_started = Instant::now();
        let affected_ids = self.affected_workflow_slice(root_ids)?;
        add_timing(
            &mut timings,
            "workflow.compute_affected_slice_ms",
            affected_started,
        );
        timings.insert(
            "workflow.incremental_root_count".to_string(),
            root_ids.len() as u64,
        );
        timings.insert(
            "workflow.incremental_affected_count".to_string(),
            affected_ids.len() as u64,
        );
        self.recompute_workflow_facts_for_ids_with_timings(
            &affected_ids.into_iter().collect::<Vec<_>>(),
            progress.then_some(changed_at),
            Some(&mut timings),
        )?;
        Ok(timings)
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
            .and_then(|schema| {
                schema.states.iter().position(|value| value == state)
            })
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
            // Always include the current id: recompute drops stale facts for
            // missing tickets and refreshes dependents via reverse edges.
            affected.insert(current);

            for edge in self.index.edges_to(&current)? {
                if edge.kind == "depends_on" {
                    queue.push_back(edge.from);
                }
            }
        }

        Ok(affected)
    }

    fn recompute_workflow_facts_for_ids_with_timings(
        &self,
        ticket_ids: &[Uuid],
        progress_at: Option<DateTime<Utc>>,
        mut timings: Option<&mut BTreeMap<String, u64>>,
    ) -> Result<(), StorageError> {
        let existing = self.index.get_workflow_facts_many(ticket_ids)?;

        for ticket_id in ticket_ids {
            let Some(ticket) = self.get_indexed(ticket_id)? else {
                self.index.remove_workflow_facts(ticket_id)?;
                continue;
            };

            let dependency_edges_started = Instant::now();
            let dependency_ids = self
                .index
                .edges_from(ticket_id)?
                .into_iter()
                .filter(|edge| edge.kind == "depends_on")
                .map(|edge| edge.to)
                .collect::<Vec<_>>();

            if let Some(map) = timings.as_deref_mut() {
                add_timing(
                    map,
                    "workflow.fetch_dependency_edges_ms",
                    dependency_edges_started,
                );
            }

            let dependency_fetch_started = Instant::now();
            let dependencies = self.get_indexed_many(&dependency_ids)?;
            if let Some(map) = timings.as_deref_mut() {
                add_timing(
                    map,
                    "workflow.fetch_dependency_tickets_ms",
                    dependency_fetch_started,
                );
            }

            let unresolved_started = Instant::now();
            let unresolved_dependency_count = dependency_ids
                .iter()
                .filter(|dependency_id| {
                    dependencies
                        .get(dependency_id)
                        .map(|dependency| {
                            !is_done_state(dependency.state.as_deref())
                        })
                        .unwrap_or(true)
                })
                .count();

            if let Some(map) = timings.as_deref_mut() {
                add_timing(
                    map,
                    "workflow.compute_unresolved_ms",
                    unresolved_started,
                );
            }

            let old_facts = existing.get(ticket_id);
            let became_actionable_at = if unresolved_dependency_count == 0 {
                match old_facts {
                    Some(facts) if facts.unresolved_dependency_count > 0 =>
                        Some(progress_at.unwrap_or(ticket.updated_at)),
                    Some(facts) =>
                        facts.became_actionable_at.or(Some(ticket.created_at)),
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

            let write_started = Instant::now();
            self.index.insert_workflow_facts(
                ticket_id,
                &WorkflowFacts {
                    unresolved_dependency_count,
                    became_actionable_at,
                    last_blocker_progress_at,
                },
            )?;

            if let Some(map) = timings.as_deref_mut() {
                add_timing(map, "workflow.write_facts_ms", write_started);
            }
        }

        Ok(())
    }
}

fn add_timing(
    timings: &mut BTreeMap<String, u64>,
    key: &str,
    started: Instant,
) {
    let elapsed = started.elapsed().as_millis() as u64;
    *timings.entry(key.to_string()).or_insert(0) += elapsed;
}

fn is_done_state(state: Option<&str>) -> bool {
    matches!(state, Some("done" | "cancelled"))
}

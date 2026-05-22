//! Shared dependency-convergence model for ticket ranking, health, and audit.
//!
//! `WorkflowModel` is the canonical place to derive reverse-dependency pressure
//! and dependency-state inversion evidence. Consumers should use this module
//! instead of reimplementing graph traversal or state-gap logic so `ticket
//! next`, `ticket-mcp next_tickets`, ticket health surfaces, and repo audit
//! stay aligned.

use std::cmp::Ordering;
use std::collections::{
    HashMap,
    HashSet,
    VecDeque,
};

use uuid::Uuid;

use crate::{
    error::StorageError,
    model::edge::EdgeRecord,
    storage::{
        indexed::IndexedTicket,
        store::TicketStore,
        ticket_fs::TicketFs,
    },
};

const DONE_STATES: &[&str] = &["done", "cancelled"];
const PAUSED_STATES: &[&str] = &["on-hold"];

/// Derived ranking and explainability fields for one ticket candidate.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TicketConvergenceMetrics {
    pub dependency_count: usize,
    pub immediate_dependees: usize,
    pub transitive_reverse_dependents: usize,
    pub affected_reverse_dependent_reach: usize,
    pub max_affected_dependent_state: Option<String>,
    pub max_affected_dependent_state_index: Option<usize>,
    pub dependency_state_gap: usize,
}

/// Evidence that a prerequisite is lagging behind a more advanced dependent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyStateInversion {
    pub dependent_id: Uuid,
    pub dependent_title: Option<String>,
    pub dependent_state: Option<String>,
    pub prerequisite_id: Uuid,
    pub prerequisite_title: Option<String>,
    pub prerequisite_state: Option<String>,
    pub dependency_state_gap: usize,
    pub affected_reverse_dependent_reach: usize,
    pub transitive_reverse_dependents: usize,
}

/// Canonical dependency-convergence graph model shared across ticket surfaces.
pub struct WorkflowModel {
    tickets: HashMap<Uuid, IndexedTicket>,
    state_index: HashMap<String, usize>,
    priorities: HashMap<Uuid, String>,
    dependency_counts: HashMap<Uuid, usize>,
    dependee_counts: HashMap<Uuid, usize>,
    unresolved_deps: HashMap<Uuid, Vec<Uuid>>,
    reverse_map: HashMap<Uuid, Vec<Uuid>>,
    metrics: HashMap<Uuid, TicketConvergenceMetrics>,
    inversions_by_dependent: HashMap<Uuid, Vec<DependencyStateInversion>>,
}

impl WorkflowModel {
    /// Build the shared workflow model from indexed tickets and dependency edges.
    pub fn build(
        store: &TicketStore,
        tickets: Vec<IndexedTicket>,
        all_edges: Vec<EdgeRecord>,
    ) -> Result<Self, StorageError> {
        let state_index = build_state_index(store);
        let priorities = read_priorities(&tickets);
        Ok(Self::build_from_parts(
            tickets,
            all_edges,
            state_index,
            priorities,
        ))
    }

    pub fn ticket(
        &self,
        ticket_id: &Uuid,
    ) -> Option<&IndexedTicket> {
        self.tickets.get(ticket_id)
    }

    pub fn priority(
        &self,
        ticket_id: &Uuid,
    ) -> Option<&str> {
        self.priorities.get(ticket_id).map(String::as_str)
    }

    pub fn priority_or_none(
        &self,
        ticket_id: &Uuid,
    ) -> &str {
        self.priority(ticket_id).unwrap_or("none")
    }

    pub fn metrics(
        &self,
        ticket_id: &Uuid,
    ) -> Option<&TicketConvergenceMetrics> {
        self.metrics.get(ticket_id)
    }

    pub fn dependency_count(
        &self,
        ticket_id: &Uuid,
    ) -> usize {
        self.dependency_counts.get(ticket_id).copied().unwrap_or(0)
    }

    pub fn dependee_count(
        &self,
        ticket_id: &Uuid,
    ) -> usize {
        self.dependee_counts.get(ticket_id).copied().unwrap_or(0)
    }

    pub fn unresolved_dependencies(
        &self,
        ticket_id: &Uuid,
    ) -> Option<&[Uuid]> {
        self.unresolved_deps.get(ticket_id).map(Vec::as_slice)
    }

    pub fn actionable_candidate_ids(
        &self,
        scope: Option<&HashSet<Uuid>>,
    ) -> Vec<Uuid> {
        self.actionable_candidate_ids_with_satisfied(scope, &HashSet::new())
    }

    /// Return actionable candidates while treating selected ticket ids as satisfied.
    ///
    /// Root-scoped `ticket next <id>` and `unblocked-by <id>` use this to rank
    /// the remaining blocker work beneath a prerequisite without requiring the
    /// root ticket to be completed first.
    pub fn actionable_candidate_ids_with_satisfied(
        &self,
        scope: Option<&HashSet<Uuid>>,
        satisfied_ids: &HashSet<Uuid>,
    ) -> Vec<Uuid> {
        self.eligible_candidate_ids(scope)
            .into_iter()
            .filter(|ticket_id| {
                self.unresolved_dependencies_excluding(ticket_id, satisfied_ids)
                    .is_empty()
            })
            .collect()
    }

    pub fn eligible_candidate_ids(
        &self,
        scope: Option<&HashSet<Uuid>>,
    ) -> Vec<Uuid> {
        self.tickets
            .values()
            .filter(|ticket| scope.is_none_or(|ids| ids.contains(&ticket.id)))
            .filter(|ticket| is_candidate_state(ticket.state.as_deref()))
            .map(|ticket| ticket.id)
            .collect()
    }

    pub fn sort_candidate_ids(
        &self,
        candidates: &mut [Uuid],
    ) {
        candidates.sort_by(|left, right| self.compare_candidate_ids(*left, *right));
    }

    /// Collect all transitive reverse dependents that directly or indirectly
    /// rely on the supplied ticket.
    pub fn reverse_dependents(
        &self,
        root_id: Uuid,
    ) -> HashSet<Uuid> {
        let mut visited = HashSet::new();
        let mut dependents = HashSet::new();
        let mut queue = VecDeque::from([root_id]);

        while let Some(current) = queue.pop_front() {
            if !visited.insert(current) {
                continue;
            }

            for dependent in self.reverse_map.get(&current).into_iter().flatten() {
                if dependents.insert(*dependent) {
                    queue.push_back(*dependent);
                }
            }
        }

        dependents.remove(&root_id);
        dependents
    }

    pub fn remaining_blockers_for_dependents(
        &self,
        dependent_ids: &HashSet<Uuid>,
    ) -> HashSet<Uuid> {
        self.remaining_blockers_for_dependents_with_satisfied(
            dependent_ids,
            &HashSet::new(),
        )
    }

    /// Return the unresolved prerequisite ids for a dependent set while
    /// treating selected tickets as already satisfied.
    pub fn remaining_blockers_for_dependents_with_satisfied(
        &self,
        dependent_ids: &HashSet<Uuid>,
        satisfied_ids: &HashSet<Uuid>,
    ) -> HashSet<Uuid> {
        dependent_ids
            .iter()
            .flat_map(|ticket_id| {
                self.unresolved_dependencies_excluding(ticket_id, satisfied_ids)
            })
            .collect()
    }

    pub fn unresolved_dependencies_excluding(
        &self,
        ticket_id: &Uuid,
        satisfied_ids: &HashSet<Uuid>,
    ) -> Vec<Uuid> {
        self.unresolved_deps
            .get(ticket_id)
            .into_iter()
            .flatten()
            .filter(|dependency_id| !satisfied_ids.contains(dependency_id))
            .copied()
            .collect()
    }

    /// Return the direct dependency-state inversions for one dependent ticket.
    pub fn dependency_state_inversions(
        &self,
        dependent_id: &Uuid,
    ) -> Option<&[DependencyStateInversion]> {
        self.inversions_by_dependent.get(dependent_id).map(Vec::as_slice)
    }

    pub fn state_rank(
        &self,
        state: Option<&str>,
    ) -> usize {
        state
            .and_then(|value| self.state_index.get(value).copied())
            .unwrap_or(0)
    }

    fn build_from_parts(
        tickets: Vec<IndexedTicket>,
        all_edges: Vec<EdgeRecord>,
        state_index: HashMap<String, usize>,
        priorities: HashMap<Uuid, String>,
    ) -> Self {
        let tickets: HashMap<Uuid, IndexedTicket> = tickets
            .into_iter()
            .map(|ticket| (ticket.id, ticket))
            .collect();
        let dependency_counts = dependency_counts(&all_edges);
        let dependee_counts = dependee_counts(&all_edges);
        let unresolved_deps = unresolved_dependency_map(&tickets, &all_edges);
        let reverse_map = reverse_map(&all_edges);
        let metrics = compute_metrics(
            &tickets,
            &state_index,
            &dependency_counts,
            &dependee_counts,
            &reverse_map,
        );
        let inversions_by_dependent = compute_dependency_state_inversions(
            &tickets,
            &all_edges,
            &state_index,
            &metrics,
        );

        Self {
            tickets,
            state_index,
            priorities,
            dependency_counts,
            dependee_counts,
            unresolved_deps,
            reverse_map,
            metrics,
            inversions_by_dependent,
        }
    }

    fn compare_candidate_ids(
        &self,
        left: Uuid,
        right: Uuid,
    ) -> Ordering {
        let Some(left_ticket) = self.tickets.get(&left) else {
            return Ordering::Greater;
        };
        let Some(right_ticket) = self.tickets.get(&right) else {
            return Ordering::Less;
        };
        let left_metrics = self.metrics.get(&left).cloned().unwrap_or_default();
        let right_metrics = self.metrics.get(&right).cloned().unwrap_or_default();

        right_metrics
            .max_affected_dependent_state_index
            .unwrap_or(0)
            .cmp(&left_metrics.max_affected_dependent_state_index.unwrap_or(0))
            .then_with(|| {
                right_metrics
                    .dependency_state_gap
                    .cmp(&left_metrics.dependency_state_gap)
            })
            .then_with(|| {
                right_metrics
                    .affected_reverse_dependent_reach
                    .cmp(&left_metrics.affected_reverse_dependent_reach)
            })
            .then_with(|| {
                priority_weight(self.priority_or_none(&left))
                    .cmp(&priority_weight(self.priority_or_none(&right)))
            })
            .then_with(|| {
                self.state_rank(right_ticket.state.as_deref())
                    .cmp(&self.state_rank(left_ticket.state.as_deref()))
            })
            .then_with(|| {
                right_metrics
                    .transitive_reverse_dependents
                    .cmp(&left_metrics.transitive_reverse_dependents)
            })
            .then_with(|| {
                right_metrics
                    .immediate_dependees
                    .cmp(&left_metrics.immediate_dependees)
            })
            .then_with(|| right_ticket.created_at.cmp(&left_ticket.created_at))
            .then_with(|| ticket_title(left_ticket).cmp(ticket_title(right_ticket)))
            .then_with(|| left.cmp(&right))
    }
}

fn build_state_index(store: &TicketStore) -> HashMap<String, usize> {
    let mut state_index = HashMap::new();
    for type_id in store.schema_registry().type_ids() {
        if let Some(schema) = store.schema_registry().get(type_id) {
            for (index, state) in schema.states.iter().enumerate() {
                state_index.entry(state.clone()).or_insert(index);
            }
        }
    }
    state_index
}

fn read_priorities(tickets: &[IndexedTicket]) -> HashMap<Uuid, String> {
    tickets
        .iter()
        .filter_map(|ticket| {
            TicketFs::read(&ticket.path).ok().and_then(|manifest| {
                manifest
                    .extra
                    .get("priority")
                    .and_then(|value| value.as_str())
                    .map(|priority| (ticket.id, priority.to_string()))
            })
        })
        .collect()
}

fn dependency_counts(all_edges: &[EdgeRecord]) -> HashMap<Uuid, usize> {
    let mut counts = HashMap::new();
    for edge in all_edges {
        if edge.kind == "depends_on" {
            *counts.entry(edge.from).or_insert(0) += 1;
        }
    }
    counts
}

fn dependee_counts(all_edges: &[EdgeRecord]) -> HashMap<Uuid, usize> {
    let mut counts = HashMap::new();
    for edge in all_edges {
        if edge.kind == "depends_on" {
            *counts.entry(edge.to).or_insert(0) += 1;
        }
    }
    counts
}

fn unresolved_dependency_map(
    tickets: &HashMap<Uuid, IndexedTicket>,
    all_edges: &[EdgeRecord],
) -> HashMap<Uuid, Vec<Uuid>> {
    let mut unresolved = HashMap::new();
    for edge in all_edges {
        if edge.kind != "depends_on" {
            continue;
        }
        let is_resolved = tickets
            .get(&edge.to)
            .map(|ticket| is_done_state(ticket.state.as_deref()))
            .unwrap_or(false);
        if !is_resolved {
            unresolved.entry(edge.from).or_insert_with(Vec::new).push(edge.to);
        }
    }
    unresolved
}

fn reverse_map(all_edges: &[EdgeRecord]) -> HashMap<Uuid, Vec<Uuid>> {
    let mut reverse_map = HashMap::new();
    for edge in all_edges {
        if edge.kind == "depends_on" {
            reverse_map.entry(edge.to).or_insert_with(Vec::new).push(edge.from);
        }
    }
    reverse_map
}

fn compute_metrics(
    tickets: &HashMap<Uuid, IndexedTicket>,
    state_index: &HashMap<String, usize>,
    dependency_counts: &HashMap<Uuid, usize>,
    dependee_counts: &HashMap<Uuid, usize>,
    reverse_map: &HashMap<Uuid, Vec<Uuid>>,
) -> HashMap<Uuid, TicketConvergenceMetrics> {
    tickets
        .keys()
        .map(|ticket_id| {
            let transitive_ids = reverse_dependents_for(*ticket_id, reverse_map);
            let affected_ids: Vec<Uuid> = transitive_ids
                .iter()
                .filter(|dependent_id| {
                    tickets
                        .get(dependent_id)
                        .map(|ticket| !is_done_state(ticket.state.as_deref()))
                        .unwrap_or(false)
                })
                .copied()
                .collect();
            let max_affected = affected_ids
                .iter()
                .filter_map(|dependent_id| tickets.get(dependent_id))
                .filter_map(|ticket| {
                    let state = ticket.state.clone();
                    let index = state
                        .as_deref()
                        .and_then(|value| state_index.get(value).copied());
                    index.map(|index| (state, index))
                })
                .max_by_key(|(_, index)| *index);
            let ticket_state_index = tickets
                .get(ticket_id)
                .and_then(|ticket| {
                    ticket
                        .state
                        .as_deref()
                        .and_then(|value| state_index.get(value).copied())
                })
                .unwrap_or(0);
            let (max_affected_dependent_state, max_affected_dependent_state_index) =
                match max_affected {
                    Some((state, index)) => (state, Some(index)),
                    None => (None, None),
                };

            (
                *ticket_id,
                TicketConvergenceMetrics {
                    dependency_count: dependency_counts.get(ticket_id).copied().unwrap_or(0),
                    immediate_dependees: dependee_counts.get(ticket_id).copied().unwrap_or(0),
                    transitive_reverse_dependents: transitive_ids.len(),
                    affected_reverse_dependent_reach: affected_ids.len(),
                    max_affected_dependent_state: max_affected_dependent_state.clone(),
                    max_affected_dependent_state_index,
                    dependency_state_gap: max_affected_dependent_state_index
                        .map(|index| index.saturating_sub(ticket_state_index))
                        .unwrap_or(0),
                },
            )
        })
        .collect()
}

fn reverse_dependents_for(
    root_id: Uuid,
    reverse_map: &HashMap<Uuid, Vec<Uuid>>,
) -> HashSet<Uuid> {
    let mut visited = HashSet::new();
    let mut dependents = HashSet::new();
    let mut queue = VecDeque::from([root_id]);

    while let Some(current) = queue.pop_front() {
        if !visited.insert(current) {
            continue;
        }

        for dependent in reverse_map.get(&current).into_iter().flatten() {
            if dependents.insert(*dependent) {
                queue.push_back(*dependent);
            }
        }
    }

    dependents.remove(&root_id);
    dependents
}

fn compute_dependency_state_inversions(
    tickets: &HashMap<Uuid, IndexedTicket>,
    all_edges: &[EdgeRecord],
    state_index: &HashMap<String, usize>,
    metrics: &HashMap<Uuid, TicketConvergenceMetrics>,
) -> HashMap<Uuid, Vec<DependencyStateInversion>> {
    let mut inversions = HashMap::<Uuid, Vec<DependencyStateInversion>>::new();

    for edge in all_edges {
        if edge.kind != "depends_on" {
            continue;
        }
        let Some(dependent) = tickets.get(&edge.from) else {
            continue;
        };
        let Some(prerequisite) = tickets.get(&edge.to) else {
            continue;
        };
        if is_done_state(dependent.state.as_deref())
            || is_done_state(prerequisite.state.as_deref())
        {
            continue;
        }

        let dependent_index = dependent
            .state
            .as_deref()
            .and_then(|state| state_index.get(state).copied())
            .unwrap_or(0);
        let prerequisite_index = prerequisite
            .state
            .as_deref()
            .and_then(|state| state_index.get(state).copied())
            .unwrap_or(0);
        if dependent_index <= prerequisite_index {
            continue;
        }

        let prerequisite_metrics = metrics.get(&prerequisite.id).cloned().unwrap_or_default();
        inversions
            .entry(dependent.id)
            .or_insert_with(Vec::new)
            .push(DependencyStateInversion {
                dependent_id: dependent.id,
                dependent_title: dependent.title.clone(),
                dependent_state: dependent.state.clone(),
                prerequisite_id: prerequisite.id,
                prerequisite_title: prerequisite.title.clone(),
                prerequisite_state: prerequisite.state.clone(),
                dependency_state_gap: dependent_index.saturating_sub(prerequisite_index),
                affected_reverse_dependent_reach: prerequisite_metrics.affected_reverse_dependent_reach,
                transitive_reverse_dependents: prerequisite_metrics.transitive_reverse_dependents,
            });
    }

    for issues in inversions.values_mut() {
        issues.sort_by(|left, right| {
            right
                .dependency_state_gap
                .cmp(&left.dependency_state_gap)
                .then_with(|| {
                    right
                        .affected_reverse_dependent_reach
                        .cmp(&left.affected_reverse_dependent_reach)
                })
                .then_with(|| left.prerequisite_id.cmp(&right.prerequisite_id))
        });
    }

    inversions
}

fn is_done_state(state: Option<&str>) -> bool {
    matches!(state, Some("done" | "cancelled"))
}

fn is_candidate_state(state: Option<&str>) -> bool {
    state
        .map(|value| !DONE_STATES.contains(&value) && !PAUSED_STATES.contains(&value))
        .unwrap_or(true)
}

fn ticket_title(ticket: &IndexedTicket) -> &str {
    ticket.title.as_deref().unwrap_or("")
}

fn priority_weight(priority: &str) -> u8 {
    match priority {
        "critical" => 0,
        "high" => 1,
        "medium" => 2,
        "low" => 3,
        "backlog" => 5,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::{
        TimeZone,
        Utc,
    };

    use super::*;

    fn ticket(
        title: &str,
        state: &str,
        created_at: chrono::DateTime<Utc>,
    ) -> IndexedTicket {
        IndexedTicket {
            id: Uuid::new_v4(),
            path: PathBuf::from(title),
            type_id: "tracker-improvement".to_string(),
            title: Some(title.to_string()),
            state: Some(state.to_string()),
            created_at,
            updated_at: created_at,
            deleted: false,
        }
    }

    fn build_model(
        tickets: Vec<IndexedTicket>,
        edges: Vec<EdgeRecord>,
        priorities: HashMap<Uuid, String>,
    ) -> WorkflowModel {
        WorkflowModel::build_from_parts(
            tickets,
            edges,
            HashMap::from([
                ("new".to_string(), 0usize),
                ("ready".to_string(), 1usize),
                ("in-implementation".to_string(), 2usize),
                ("in-review".to_string(), 3usize),
                ("done".to_string(), 4usize),
            ]),
            priorities,
        )
    }

    #[test]
    fn sort_candidates_prefers_newer_tickets_before_older_ones_without_pressure() {
        let older = ticket(
            "Older ticket",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
        );
        let newer = ticket(
            "Newer ticket",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 17, 12, 0, 0).unwrap(),
        );
        let mut candidates = vec![older.id, newer.id];
        let model = build_model(
            vec![older.clone(), newer.clone()],
            Vec::new(),
            HashMap::from([
                (older.id, "high".to_string()),
                (newer.id, "high".to_string()),
            ]),
        );

        model.sort_candidate_ids(&mut candidates);

        assert_eq!(candidates, vec![newer.id, older.id]);
    }

    #[test]
    fn sort_candidates_prefers_more_dependees_before_newer_tickets_without_pressure() {
        let older = ticket(
            "Older blocker",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
        );
        let newer = ticket(
            "Newer blocker",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 17, 12, 0, 0).unwrap(),
        );
        let dependent_one = ticket(
            "Dependent one",
            "new",
            Utc.with_ymd_and_hms(2026, 5, 18, 12, 0, 0).unwrap(),
        );
        let dependent_two = ticket(
            "Dependent two",
            "new",
            Utc.with_ymd_and_hms(2026, 5, 18, 12, 30, 0).unwrap(),
        );
        let now = Utc.with_ymd_and_hms(2026, 5, 18, 13, 0, 0).unwrap();
        let mut candidates = vec![newer.id, older.id];
        let model = build_model(
            vec![older.clone(), newer.clone(), dependent_one.clone(), dependent_two.clone()],
            vec![
                EdgeRecord {
                    from: dependent_one.id,
                    to: older.id,
                    kind: "depends_on".to_string(),
                    created_at: now,
                },
                EdgeRecord {
                    from: dependent_two.id,
                    to: older.id,
                    kind: "depends_on".to_string(),
                    created_at: now,
                },
            ],
            HashMap::from([
                (older.id, "high".to_string()),
                (newer.id, "high".to_string()),
            ]),
        );

        model.sort_candidate_ids(&mut candidates);

        assert_eq!(candidates, vec![older.id, newer.id]);
    }

    #[test]
    fn convergence_pressure_promotes_earlier_state_prerequisite() {
        let prerequisite = ticket(
            "Lagging prerequisite",
            "new",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
        );
        let unrelated = ticket(
            "Unrelated ready work",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 17, 12, 0, 0).unwrap(),
        );
        let dependent = ticket(
            "Advanced dependent",
            "in-review",
            Utc.with_ymd_and_hms(2026, 5, 18, 12, 0, 0).unwrap(),
        );
        let now = Utc.with_ymd_and_hms(2026, 5, 18, 13, 0, 0).unwrap();
        let mut candidates = vec![unrelated.id, prerequisite.id];
        let model = build_model(
            vec![prerequisite.clone(), unrelated.clone(), dependent.clone()],
            vec![EdgeRecord {
                from: dependent.id,
                to: prerequisite.id,
                kind: "depends_on".to_string(),
                created_at: now,
            }],
            HashMap::from([
                (prerequisite.id, "high".to_string()),
                (unrelated.id, "high".to_string()),
            ]),
        );

        model.sort_candidate_ids(&mut candidates);

        assert_eq!(candidates, vec![prerequisite.id, unrelated.id]);
        let metrics = model.metrics(&prerequisite.id).expect("metrics");
        assert_eq!(metrics.affected_reverse_dependent_reach, 1);
        assert_eq!(metrics.max_affected_dependent_state.as_deref(), Some("in-review"));
        assert_eq!(metrics.dependency_state_gap, 3);
    }

    #[test]
    fn reverse_dependents_collect_transitive_dependents() {
        let root = ticket(
            "Root",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
        );
        let direct = ticket(
            "Direct dependent",
            "new",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 30, 0).unwrap(),
        );
        let transitive = ticket(
            "Transitive dependent",
            "new",
            Utc.with_ymd_and_hms(2026, 5, 16, 13, 0, 0).unwrap(),
        );
        let now = Utc.with_ymd_and_hms(2026, 5, 16, 13, 30, 0).unwrap();
        let model = build_model(
            vec![root.clone(), direct.clone(), transitive.clone()],
            vec![
                EdgeRecord {
                    from: direct.id,
                    to: root.id,
                    kind: "depends_on".to_string(),
                    created_at: now,
                },
                EdgeRecord {
                    from: transitive.id,
                    to: direct.id,
                    kind: "depends_on".to_string(),
                    created_at: now,
                },
            ],
            HashMap::new(),
        );

        let dependents = model.reverse_dependents(root.id);

        assert!(dependents.contains(&direct.id));
        assert!(dependents.contains(&transitive.id));
    }

    #[test]
    fn dependency_state_inversions_capture_more_advanced_dependents() {
        let prerequisite = ticket(
            "Lagging prerequisite",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
        );
        let dependent = ticket(
            "Advanced dependent",
            "in-review",
            Utc.with_ymd_and_hms(2026, 5, 17, 12, 0, 0).unwrap(),
        );
        let now = Utc.with_ymd_and_hms(2026, 5, 18, 12, 0, 0).unwrap();
        let model = build_model(
            vec![prerequisite.clone(), dependent.clone()],
            vec![EdgeRecord {
                from: dependent.id,
                to: prerequisite.id,
                kind: "depends_on".to_string(),
                created_at: now,
            }],
            HashMap::new(),
        );

        let inversions = model
            .dependency_state_inversions(&dependent.id)
            .expect("dependency inversion");

        assert_eq!(inversions.len(), 1);
        assert_eq!(inversions[0].prerequisite_id, prerequisite.id);
        assert_eq!(inversions[0].dependent_id, dependent.id);
        assert_eq!(inversions[0].dependency_state_gap, 2);
        assert_eq!(inversions[0].affected_reverse_dependent_reach, 1);
    }
}
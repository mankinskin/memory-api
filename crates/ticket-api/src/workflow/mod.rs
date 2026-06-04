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

use chrono::{
    DateTime,
    Utc,
};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    BoardEntryStatus,
    BoardSnapshot,
    error::StorageError,
    model::edge::EdgeRecord,
    storage::{
        indexed::{
            IndexedTicket,
            WorkflowFacts,
        },
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
    pub became_actionable_at: Option<DateTime<Utc>>,
    pub last_blocker_progress_at: Option<DateTime<Utc>>,
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

/// Nested workflow tree node used by blocker and unlock exploration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowTreeNode {
    pub ticket_id: Uuid,
    pub title: Option<String>,
    pub state: Option<String>,
    pub priority: String,
    pub children: Vec<WorkflowTreeNode>,
    pub remaining_blocker_count: usize,
    pub unresolved_frontier_leaf_count: usize,
    pub frontier_leaf_ids: Vec<Uuid>,
    pub blocker_distance: usize,
    pub is_frontier: bool,
    pub dependency_count: usize,
    pub immediate_dependees: usize,
    pub transitive_reverse_dependents: usize,
    pub affected_reverse_dependent_reach: usize,
    pub dependency_state_gap: usize,
}

/// Board-owned ticket surfaced separately from visible workflow candidates.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BoardExcludedCandidate {
    pub ticket_id: Uuid,
    pub agent_id: String,
    pub status: String,
    pub intent: String,
}

/// Board-aware candidate view used by `next` surfaces.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoardAwareCandidates {
    pub candidates: Vec<Uuid>,
    pub excluded_by_board: Vec<BoardExcludedCandidate>,
    pub warnings: Vec<String>,
}

/// Canonical dependency-convergence graph model shared across ticket surfaces.
pub struct WorkflowModel {
    tickets: HashMap<Uuid, IndexedTicket>,
    state_index: HashMap<String, usize>,
    priorities: HashMap<Uuid, String>,
    efforts: HashMap<Uuid, u64>,
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
        let efforts = read_efforts(&tickets);
        let workflow_facts = store
            .get_workflow_facts_many(
                &tickets.iter().map(|ticket| ticket.id).collect::<Vec<_>>(),
            )?;
        Ok(Self::build_from_parts(
            tickets,
            all_edges,
            state_index,
            priorities,
            efforts,
            workflow_facts,
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

    pub fn effort(
        &self,
        ticket_id: &Uuid,
    ) -> Option<u64> {
        self.efforts.get(ticket_id).copied()
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

    /// Return the set of ticket IDs whose title starts with `filter`, or `None`
    /// when no filter is supplied.  Adapters should call this instead of
    /// re-implementing title-prefix filtering locally.
    pub fn filter_scope(
        tickets: &[IndexedTicket],
        filter: Option<&str>,
    ) -> Option<HashSet<Uuid>> {
        filter.map(|prefix| {
            tickets
                .iter()
                .filter(|t| t.title.as_deref().unwrap_or("").starts_with(prefix))
                .map(|t| t.id)
                .collect()
        })
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

    /// Build an upstream blocker tree from unresolved `depends_on` edges.
    pub fn blocker_tree(
        &self,
        root_id: Uuid,
    ) -> Option<WorkflowTreeNode> {
        let mut path = HashSet::new();
        self.build_blocker_tree_node(root_id, &mut path)
    }

    /// Return the frontier leaf ids for an upstream blocker tree.
    pub fn blocker_frontier_leaf_ids(
        &self,
        root_id: Uuid,
    ) -> Vec<Uuid> {
        self.blocker_tree(root_id)
            .map(|tree| tree.frontier_leaf_ids)
            .unwrap_or_default()
    }

    /// Build a downstream unlock tree while treating the supplied ids as satisfied.
    pub fn unlock_tree_with_satisfied(
        &self,
        root_id: Uuid,
        satisfied_ids: &HashSet<Uuid>,
    ) -> Option<WorkflowTreeNode> {
        let mut path = HashSet::new();
        self.build_unlock_tree_node(root_id, satisfied_ids, false, &mut path)
    }

    /// Return the frontier leaf ids for a downstream unlock tree while
    /// treating the supplied ids as satisfied.
    pub fn unlock_frontier_leaf_ids_with_satisfied(
        &self,
        root_id: Uuid,
        satisfied_ids: &HashSet<Uuid>,
    ) -> Vec<Uuid> {
        self.unlock_tree_with_satisfied(root_id, satisfied_ids)
            .map(|tree| tree.frontier_leaf_ids)
            .unwrap_or_default()
    }

    /// Build a downstream unlock tree while treating the root id as satisfied.
    pub fn unlock_tree(
        &self,
        root_id: Uuid,
    ) -> Option<WorkflowTreeNode> {
        let satisfied_ids = HashSet::from([root_id]);
        self.unlock_tree_with_satisfied(root_id, &satisfied_ids)
    }

    /// Return the frontier leaf ids for a downstream unlock tree while
    /// treating the root id as satisfied.
    pub fn unlock_frontier_leaf_ids(
        &self,
        root_id: Uuid,
    ) -> Vec<Uuid> {
        let satisfied_ids = HashSet::from([root_id]);
        self.unlock_frontier_leaf_ids_with_satisfied(root_id, &satisfied_ids)
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

    fn build_blocker_tree_node(
        &self,
        ticket_id: Uuid,
        path: &mut HashSet<Uuid>,
    ) -> Option<WorkflowTreeNode> {
        if !self.tickets.contains_key(&ticket_id) {
            return None;
        }
        if !path.insert(ticket_id) {
            return self.finalize_tree_node(ticket_id, 0, false, Vec::new(), 1);
        }

        let child_ids = self.unresolved_dependencies_excluding(&ticket_id, &HashSet::new());
        let remaining_blocker_count = child_ids.len();
        let children = child_ids
            .into_iter()
            .filter_map(|child_id| self.build_blocker_tree_node(child_id, path))
            .collect::<Vec<_>>();

        path.remove(&ticket_id);

        self.finalize_tree_node(
            ticket_id,
            remaining_blocker_count,
            remaining_blocker_count == 0,
            children,
            remaining_blocker_count.max(1),
        )
    }

    fn build_unlock_tree_node(
        &self,
        ticket_id: Uuid,
        satisfied_ids: &HashSet<Uuid>,
        allow_frontier: bool,
        path: &mut HashSet<Uuid>,
    ) -> Option<WorkflowTreeNode> {
        let ticket = self.tickets.get(&ticket_id)?;
        if !path.insert(ticket_id) {
            return self.finalize_tree_node(ticket_id, 0, false, Vec::new(), 1);
        }

        let child_ids = self
            .reverse_map
            .get(&ticket_id)
            .into_iter()
            .flatten()
            .filter(|child_id| {
                self.tickets
                    .get(child_id)
                    .map(|child| is_candidate_state(child.state.as_deref()))
                    .unwrap_or(false)
            })
            .copied()
            .collect::<Vec<_>>();
        let remaining_blocker_count =
            self.unresolved_dependencies_excluding(&ticket_id, satisfied_ids).len();
        let is_frontier = allow_frontier
            && remaining_blocker_count == 0
            && is_candidate_state(ticket.state.as_deref());
        let children = child_ids
            .into_iter()
            .filter_map(|child_id| {
                self.build_unlock_tree_node(child_id, satisfied_ids, true, path)
            })
            .collect::<Vec<_>>();

        path.remove(&ticket_id);

        self.finalize_tree_node(
            ticket_id,
            remaining_blocker_count,
            is_frontier,
            children,
            remaining_blocker_count.max(1),
        )
    }

    fn finalize_tree_node(
        &self,
        ticket_id: Uuid,
        remaining_blocker_count: usize,
        is_frontier: bool,
        mut children: Vec<WorkflowTreeNode>,
        fallback_distance: usize,
    ) -> Option<WorkflowTreeNode> {
        let ticket = self.tickets.get(&ticket_id)?;
        self.sort_tree_nodes(&mut children);

        let frontier_leaf_ids = if is_frontier {
            vec![ticket_id]
        } else if children.is_empty() {
            vec![ticket_id]
        } else {
            children
                .iter()
                .flat_map(|child| child.frontier_leaf_ids.iter().copied())
                .collect()
        };
        let blocker_distance = if is_frontier {
            0
        } else if children.is_empty() {
            fallback_distance
        } else {
            children
                .iter()
                .map(|child| child.blocker_distance.saturating_add(1))
                .min()
                .unwrap_or(fallback_distance)
        };
        let metrics = self.metrics.get(&ticket_id).cloned().unwrap_or_default();

        Some(WorkflowTreeNode {
            ticket_id,
            title: ticket.title.clone(),
            state: ticket.state.clone(),
            priority: self.priority_or_none(&ticket_id).to_string(),
            children,
            remaining_blocker_count,
            unresolved_frontier_leaf_count: frontier_leaf_ids.len(),
            frontier_leaf_ids,
            blocker_distance,
            is_frontier,
            dependency_count: metrics.dependency_count,
            immediate_dependees: metrics.immediate_dependees,
            transitive_reverse_dependents: metrics.transitive_reverse_dependents,
            affected_reverse_dependent_reach: metrics.affected_reverse_dependent_reach,
            dependency_state_gap: metrics.dependency_state_gap,
        })
    }

    fn sort_tree_nodes(
        &self,
        nodes: &mut [WorkflowTreeNode],
    ) {
        nodes.sort_by(|left, right| {
            left.unresolved_frontier_leaf_count
                .cmp(&right.unresolved_frontier_leaf_count)
                .then_with(|| left.blocker_distance.cmp(&right.blocker_distance))
                .then_with(|| {
                    effort_sort_key(self.effort(&left.ticket_id))
                        .cmp(&effort_sort_key(self.effort(&right.ticket_id)))
                })
                .then_with(|| {
                    right
                        .dependency_state_gap
                        .cmp(&left.dependency_state_gap)
                })
                .then_with(|| {
                    right
                        .affected_reverse_dependent_reach
                        .cmp(&left.affected_reverse_dependent_reach)
                })
                .then_with(|| {
                    right
                        .transitive_reverse_dependents
                        .cmp(&left.transitive_reverse_dependents)
                })
                .then_with(|| priority_weight(&left.priority).cmp(&priority_weight(&right.priority)))
                .then_with(|| {
                    left.title
                        .as_deref()
                        .unwrap_or("")
                        .cmp(right.title.as_deref().unwrap_or(""))
                })
                .then_with(|| left.ticket_id.cmp(&right.ticket_id))
        });
    }

    fn build_from_parts(
        tickets: Vec<IndexedTicket>,
        all_edges: Vec<EdgeRecord>,
        state_index: HashMap<String, usize>,
        priorities: HashMap<Uuid, String>,
        efforts: HashMap<Uuid, u64>,
        workflow_facts: HashMap<Uuid, WorkflowFacts>,
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
            &workflow_facts,
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
            efforts,
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
                effort_sort_key(self.effort(&left))
                    .cmp(&effort_sort_key(self.effort(&right)))
            })
            .then_with(|| {
                right_metrics
                    .became_actionable_at
                    .cmp(&left_metrics.became_actionable_at)
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

/// Apply board-awareness on top of already-ranked actionable workflow candidates.
///
/// The returned `candidates` preserve the input order, removing tickets covered
/// by active or stale board entries unless `skip_board` is `true`.
/// Excluded tickets are surfaced separately so callers can still explain why a
/// candidate disappeared from `items`.
pub fn apply_board_filter(
    candidates: Vec<Uuid>,
    board_snap: Option<&BoardSnapshot>,
    skip_board: bool,
) -> BoardAwareCandidates {
    let warnings = board_warnings(board_snap);

    if skip_board {
        return BoardAwareCandidates {
            candidates,
            excluded_by_board: Vec::new(),
            warnings,
        };
    }

    let Some(snapshot) = board_snap else {
        return BoardAwareCandidates {
            candidates,
            excluded_by_board: Vec::new(),
            warnings,
        };
    };

    let candidate_ids: HashSet<Uuid> = candidates.iter().copied().collect();
    let excluded_by_board = snapshot
        .entries
        .iter()
        .filter(|entry| tracked_by_board(&entry.status) && candidate_ids.contains(&entry.ticket_id))
        .map(|entry| BoardExcludedCandidate {
            ticket_id: entry.ticket_id,
            agent_id: entry.agent_id.clone(),
            status: board_status(&entry.status).to_string(),
            intent: entry.intent.clone(),
        })
        .collect::<Vec<_>>();

    let blocked_ids: HashSet<Uuid> = snapshot
        .entries
        .iter()
        .filter(|entry| tracked_by_board(&entry.status))
        .map(|entry| entry.ticket_id)
        .collect();

    BoardAwareCandidates {
        candidates: candidates
            .into_iter()
            .filter(|ticket_id| !blocked_ids.contains(ticket_id))
            .collect(),
        excluded_by_board,
        warnings,
    }
}

fn board_warnings(board_snap: Option<&BoardSnapshot>) -> Vec<String> {
    let Some(snapshot) = board_snap else {
        return Vec::new();
    };

    let mut warnings = Vec::new();
    let max_wip = snapshot.config.max_wip;

    if snapshot.active_count >= max_wip {
        warnings.push(format!(
            "WIP limit reached: {}/{} active entries — pause new work and reduce the board.",
            snapshot.active_count, max_wip
        ));
    } else if max_wip > 0 && snapshot.active_count + 1 >= max_wip {
        warnings.push(format!(
            "Approaching WIP limit: {}/{} active entries.",
            snapshot.active_count, max_wip
        ));
    }

    if snapshot.stale_count > 0 {
        warnings.push(format!(
            "{} stale board entr{} — heartbeat has expired; run board heartbeat or clean.",
            snapshot.stale_count,
            if snapshot.stale_count == 1 { "y" } else { "ies" }
        ));
    }

    warnings
}

fn tracked_by_board(status: &BoardEntryStatus) -> bool {
    matches!(status, BoardEntryStatus::Active | BoardEntryStatus::Stale)
}

fn board_status(status: &BoardEntryStatus) -> &'static str {
    match status {
        BoardEntryStatus::Active => "active",
        BoardEntryStatus::Stale => "stale",
        BoardEntryStatus::Conflict => "conflict",
        BoardEntryStatus::Completed => "completed",
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

pub fn parse_effort(value: &str) -> Option<u64> {
    let compact = value.trim().to_ascii_lowercase().replace([',', '_'], "");
    let chars: Vec<char> = compact.chars().collect();
    let mut index = 0;

    while index < chars.len() {
        if chars[index].is_ascii_digit() {
            let start = index;
            let mut seen_dot = false;
            index += 1;
            while index < chars.len() {
                let ch = chars[index];
                if ch.is_ascii_digit() {
                    index += 1;
                    continue;
                }
                if ch == '.' && !seen_dot {
                    seen_dot = true;
                    index += 1;
                    continue;
                }
                break;
            }

            let number = compact[start..index].parse::<f64>().ok()?;
            let suffix = chars.get(index).copied();
            let multiplier = match suffix {
                Some('k') => 1_000_f64,
                Some('m') => 1_000_000_f64,
                Some('b') => 1_000_000_000_f64,
                _ => 1_f64,
            };
            return Some((number * multiplier).round() as u64);
        }
        index += 1;
    }

    None
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

fn read_efforts(tickets: &[IndexedTicket]) -> HashMap<Uuid, u64> {
    tickets
        .iter()
        .filter_map(|ticket| {
            TicketFs::read(&ticket.path).ok().and_then(|manifest| {
                manifest
                    .extra
                    .get("effort")
                    .and_then(|value| value.as_str())
                    .and_then(parse_effort)
                    .map(|effort| (ticket.id, effort))
            })
        })
        .collect()
}

fn effort_sort_key(effort: Option<u64>) -> u64 {
    effort.unwrap_or(u64::MAX)
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
    workflow_facts: &HashMap<Uuid, WorkflowFacts>,
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
            let facts = workflow_facts.get(ticket_id).cloned().unwrap_or_default();

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
                    became_actionable_at: facts.became_actionable_at,
                    last_blocker_progress_at: facts.last_blocker_progress_at,
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
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use chrono::{
        TimeZone,
        Utc,
    };

    use super::*;
    use crate::{
        BoardConfig,
        BoardEntry,
        BoardEntryStatus,
        BoardSnapshot,
    };

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
        build_model_with_facts(
            tickets,
            edges,
            priorities,
            HashMap::new(),
            HashMap::new(),
        )
    }

    fn build_model_with_facts(
        tickets: Vec<IndexedTicket>,
        edges: Vec<EdgeRecord>,
        priorities: HashMap<Uuid, String>,
        efforts: HashMap<Uuid, u64>,
        workflow_facts: HashMap<Uuid, WorkflowFacts>,
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
            efforts,
            workflow_facts,
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
    fn sort_candidates_prefers_more_recent_actionable_time_before_creation_time() {
        let older_created = ticket(
            "Older created but recently actionable",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
        );
        let newer_created = ticket(
            "Newer created but stale actionable",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 18, 12, 0, 0).unwrap(),
        );
        let recent_actionable_at = Utc.with_ymd_and_hms(2026, 5, 19, 12, 0, 0).unwrap();
        let stale_actionable_at = Utc.with_ymd_and_hms(2026, 5, 17, 12, 0, 0).unwrap();
        let mut candidates = vec![newer_created.id, older_created.id];
        let model = build_model_with_facts(
            vec![older_created.clone(), newer_created.clone()],
            Vec::new(),
            HashMap::from([
                (older_created.id, "high".to_string()),
                (newer_created.id, "high".to_string()),
            ]),
            HashMap::new(),
            HashMap::from([
                (
                    older_created.id,
                    WorkflowFacts {
                        unresolved_dependency_count: 0,
                        became_actionable_at: Some(recent_actionable_at),
                        last_blocker_progress_at: None,
                    },
                ),
                (
                    newer_created.id,
                    WorkflowFacts {
                        unresolved_dependency_count: 0,
                        became_actionable_at: Some(stale_actionable_at),
                        last_blocker_progress_at: None,
                    },
                ),
            ]),
        );

        model.sort_candidate_ids(&mut candidates);

        assert_eq!(candidates, vec![older_created.id, newer_created.id]);
        let metrics = model.metrics(&older_created.id).expect("metrics");
        assert_eq!(metrics.became_actionable_at, Some(recent_actionable_at));
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
    fn convergence_pressure_still_beats_recently_actionable_unrelated_work() {
        let prerequisite = ticket(
            "Lagging prerequisite",
            "new",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
        );
        let recently_actionable = ticket(
            "Recently actionable unrelated",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 18, 12, 0, 0).unwrap(),
        );
        let dependent = ticket(
            "Advanced dependent",
            "in-review",
            Utc.with_ymd_and_hms(2026, 5, 18, 13, 0, 0).unwrap(),
        );
        let now = Utc.with_ymd_and_hms(2026, 5, 18, 14, 0, 0).unwrap();
        let mut candidates = vec![recently_actionable.id, prerequisite.id];
        let model = build_model_with_facts(
            vec![
                prerequisite.clone(),
                recently_actionable.clone(),
                dependent.clone(),
            ],
            vec![EdgeRecord {
                from: dependent.id,
                to: prerequisite.id,
                kind: "depends_on".to_string(),
                created_at: now,
            }],
            HashMap::from([
                (prerequisite.id, "high".to_string()),
                (recently_actionable.id, "high".to_string()),
            ]),
            HashMap::new(),
            HashMap::from([(
                recently_actionable.id,
                WorkflowFacts {
                    unresolved_dependency_count: 0,
                    became_actionable_at: Some(now),
                    last_blocker_progress_at: None,
                },
            )]),
        );

        model.sort_candidate_ids(&mut candidates);

        assert_eq!(candidates, vec![prerequisite.id, recently_actionable.id]);
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

    #[test]
    fn blocker_tree_preserves_nested_children_and_orders_closest_frontier_first() {
        let root = ticket(
            "Root",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
        );
        let direct_leaf = ticket(
            "Direct frontier leaf",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 5, 0).unwrap(),
        );
        let nested_parent = ticket(
            "Nested parent",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 10, 0).unwrap(),
        );
        let nested_leaf = ticket(
            "Nested frontier leaf",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 15, 0).unwrap(),
        );
        let now = Utc.with_ymd_and_hms(2026, 5, 16, 13, 0, 0).unwrap();
        let model = build_model(
            vec![
                root.clone(),
                direct_leaf.clone(),
                nested_parent.clone(),
                nested_leaf.clone(),
            ],
            vec![
                EdgeRecord {
                    from: root.id,
                    to: nested_parent.id,
                    kind: "depends_on".to_string(),
                    created_at: now,
                },
                EdgeRecord {
                    from: root.id,
                    to: direct_leaf.id,
                    kind: "depends_on".to_string(),
                    created_at: now,
                },
                EdgeRecord {
                    from: nested_parent.id,
                    to: nested_leaf.id,
                    kind: "depends_on".to_string(),
                    created_at: now,
                },
            ],
            HashMap::new(),
        );

        let tree = model.blocker_tree(root.id).expect("blocker tree");

        assert_eq!(tree.remaining_blocker_count, 2);
        assert_eq!(
            tree.children
                .iter()
                .map(|child| child.ticket_id)
                .collect::<Vec<_>>(),
            vec![direct_leaf.id, nested_parent.id]
        );
        assert_eq!(tree.frontier_leaf_ids, vec![direct_leaf.id, nested_leaf.id]);
        assert_eq!(tree.unresolved_frontier_leaf_count, 2);
        assert_eq!(tree.blocker_distance, 1);
        assert!(tree.children[0].is_frontier);
        assert!(!tree.children[1].is_frontier);
        assert_eq!(tree.children[1].children.len(), 1);
        assert_eq!(tree.children[1].children[0].ticket_id, nested_leaf.id);
        assert!(tree.children[1].children[0].is_frontier);
    }

    #[test]
    fn apply_board_filter_excludes_tracked_candidates_and_surfaces_warnings() {
        let active_candidate = Uuid::new_v4();
        let free_candidate = Uuid::new_v4();
        let now = Utc.with_ymd_and_hms(2026, 6, 2, 10, 0, 0).unwrap();
        let snapshot = BoardSnapshot {
            captured_at: now,
            entries: vec![BoardEntry {
                entry_id: Uuid::new_v4(),
                ticket_id: active_candidate,
                agent_id: "parity-agent".to_string(),
                previous_attempt: None,
                checked_in_at: now,
                last_heartbeat: now,
                ttl_secs: 3600,
                intent: "in flight".to_string(),
                owned_files: Vec::new(),
                status: BoardEntryStatus::Active,
                handoff_reason: None,
                completed_at: None,
            }],
            caller_entries: Vec::new(),
            config: BoardConfig {
                max_wip: 1,
                stale_after_secs: 3600,
                completed_audit_window_secs: 3600,
            },
            active_count: 1,
            stale_count: 0,
            conflict_count: 0,
            wip_limit_reached: true,
            file_ownership: BTreeMap::new(),
            warnings: Vec::new(),
        };

        let result = apply_board_filter(
            vec![active_candidate, free_candidate],
            Some(&snapshot),
            false,
        );

        assert_eq!(result.candidates, vec![free_candidate]);
        assert_eq!(result.excluded_by_board.len(), 1);
        assert_eq!(result.excluded_by_board[0].ticket_id, active_candidate);
        assert_eq!(result.excluded_by_board[0].agent_id, "parity-agent");
        assert_eq!(result.excluded_by_board[0].status, "active");
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("WIP limit reached")),
            "expected WIP warning, got {:?}",
            result.warnings
        );
    }

    #[test]
    fn apply_board_filter_respects_skip_board_but_keeps_warnings() {
        let tracked_candidate = Uuid::new_v4();
        let now = Utc.with_ymd_and_hms(2026, 6, 2, 10, 0, 0).unwrap();
        let snapshot = BoardSnapshot {
            captured_at: now,
            entries: vec![BoardEntry {
                entry_id: Uuid::new_v4(),
                ticket_id: tracked_candidate,
                agent_id: "parity-agent".to_string(),
                previous_attempt: None,
                checked_in_at: now,
                last_heartbeat: now,
                ttl_secs: 3600,
                intent: "in flight".to_string(),
                owned_files: Vec::new(),
                status: BoardEntryStatus::Stale,
                handoff_reason: None,
                completed_at: None,
            }],
            caller_entries: Vec::new(),
            config: BoardConfig {
                max_wip: 5,
                stale_after_secs: 3600,
                completed_audit_window_secs: 3600,
            },
            active_count: 0,
            stale_count: 1,
            conflict_count: 0,
            wip_limit_reached: false,
            file_ownership: BTreeMap::new(),
            warnings: Vec::new(),
        };

        let result = apply_board_filter(vec![tracked_candidate], Some(&snapshot), true);

        assert_eq!(result.candidates, vec![tracked_candidate]);
        assert!(result.excluded_by_board.is_empty());
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("stale board entry")),
            "expected stale warning, got {:?}",
            result.warnings
        );
    }

    #[test]
    fn sort_candidates_prefers_lower_effort_before_newer_tickets() {
        let lower_effort = ticket(
            "Lower effort ticket",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
        );
        let higher_effort = ticket(
            "Higher effort ticket",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 17, 12, 0, 0).unwrap(),
        );
        let mut candidates = vec![higher_effort.id, lower_effort.id];
        let model = build_model_with_facts(
            vec![lower_effort.clone(), higher_effort.clone()],
            Vec::new(),
            HashMap::from([
                (lower_effort.id, "high".to_string()),
                (higher_effort.id, "high".to_string()),
            ]),
            HashMap::from([
                (lower_effort.id, 1_200_u64),
                (higher_effort.id, 8_000_u64),
            ]),
            HashMap::new(),
        );

        model.sort_candidate_ids(&mut candidates);

        assert_eq!(candidates, vec![lower_effort.id, higher_effort.id]);
    }

    #[test]
    fn parse_effort_accepts_numeric_token_budgets() {
        assert_eq!(parse_effort("1500"), Some(1_500));
        assert_eq!(parse_effort("2.5k tokens"), Some(2_500));
        assert_eq!(parse_effort("budget: 1_250"), Some(1_250));
        assert_eq!(parse_effort("unknown"), None);
    }

    #[test]
    fn unlock_tree_marks_actionable_parents_as_frontier_and_preserves_children() {
        let root = ticket(
            "Satisfied prerequisite",
            "done",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
        );
        let actionable_parent = ticket(
            "Actionable parent",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 5, 0).unwrap(),
        );
        let blocked_parent = ticket(
            "Blocked parent",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 10, 0).unwrap(),
        );
        let external_blocker = ticket(
            "External blocker",
            "ready",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 15, 0).unwrap(),
        );
        let grandchild = ticket(
            "Grandchild",
            "new",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 20, 0).unwrap(),
        );
        let now = Utc.with_ymd_and_hms(2026, 5, 16, 13, 0, 0).unwrap();
        let model = build_model(
            vec![
                root.clone(),
                actionable_parent.clone(),
                blocked_parent.clone(),
                external_blocker.clone(),
                grandchild.clone(),
            ],
            vec![
                EdgeRecord {
                    from: actionable_parent.id,
                    to: root.id,
                    kind: "depends_on".to_string(),
                    created_at: now,
                },
                EdgeRecord {
                    from: blocked_parent.id,
                    to: root.id,
                    kind: "depends_on".to_string(),
                    created_at: now,
                },
                EdgeRecord {
                    from: blocked_parent.id,
                    to: external_blocker.id,
                    kind: "depends_on".to_string(),
                    created_at: now,
                },
                EdgeRecord {
                    from: grandchild.id,
                    to: actionable_parent.id,
                    kind: "depends_on".to_string(),
                    created_at: now,
                },
            ],
            HashMap::new(),
        );

        let tree = model.unlock_tree(root.id).expect("unlock tree");

        assert_eq!(
            tree.children
                .iter()
                .map(|child| child.ticket_id)
                .collect::<Vec<_>>(),
            vec![actionable_parent.id, blocked_parent.id]
        );

        let actionable = &tree.children[0];
        assert!(actionable.is_frontier);
        assert_eq!(actionable.frontier_leaf_ids, vec![actionable_parent.id]);
        assert_eq!(actionable.blocker_distance, 0);
        assert_eq!(actionable.children.len(), 1);
        assert_eq!(actionable.children[0].ticket_id, grandchild.id);

        let blocked = &tree.children[1];
        assert!(!blocked.is_frontier);
        assert_eq!(blocked.remaining_blocker_count, 1);
        assert_eq!(blocked.frontier_leaf_ids, vec![blocked_parent.id]);
        assert_eq!(blocked.blocker_distance, 1);
        assert_eq!(model.unlock_frontier_leaf_ids(root.id), tree.frontier_leaf_ids);
        assert_eq!(model.blocker_frontier_leaf_ids(root.id), vec![root.id]);
    }
}
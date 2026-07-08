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

/// Shared root-scoped blocker view for `next <id>` style commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootBlockerScope {
    pub tree: WorkflowTreeNode,
    pub remaining_blockers: HashSet<Uuid>,
    pub reachable_dependencies: usize,
    pub blocked_dependencies: usize,
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

#[path = "workflow_metrics.rs"]
mod workflow_metrics;
use workflow_metrics::*;
pub use workflow_metrics::parse_effort;


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
    /// `unblocked-by <id>` uses this to rank remaining blocker work beneath a
    /// prerequisite without requiring the root ticket to be completed first.
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

    /// Build the canonical root-scoped blocker scope used by `next <id>`.
    ///
    /// This returns the unresolved blocker tree beneath the supplied root,
    /// plus descendant blocker ids and summary counts.
    pub fn root_blocker_scope(
        &self,
        root_id: Uuid,
    ) -> Option<RootBlockerScope> {
        let tree = self.blocker_tree(root_id)?;
        let remaining_blockers = collect_blocker_descendants(&tree);
        let blocked_dependencies = remaining_blockers
            .iter()
            .filter(|ticket_id| {
                !self
                    .unresolved_dependencies_excluding(ticket_id, &HashSet::new())
                    .is_empty()
            })
            .count();

        Some(RootBlockerScope {
            reachable_dependencies: remaining_blockers.len(),
            blocked_dependencies,
            remaining_blockers,
            tree,
        })
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
        let dependency_counts = dependency_counts(&tickets, &all_edges);
        let dependee_counts = dependee_counts(&tickets, &all_edges);
        let unresolved_deps = unresolved_dependency_map(&tickets, &all_edges);
        let reverse_map = reverse_map(&tickets, &all_edges);
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


#[cfg(test)]
#[path = "workflow/tests.rs"]
mod tests;

fn collect_blocker_descendants(root: &WorkflowTreeNode) -> HashSet<Uuid> {
    let mut ids = HashSet::new();
    for child in &root.children {
        collect_tree_node_ids(child, &mut ids);
    }
    ids
}

fn collect_tree_node_ids(
    node: &WorkflowTreeNode,
    ids: &mut HashSet<Uuid>,
) {
    ids.insert(node.ticket_id);
    for child in &node.children {
        collect_tree_node_ids(child, ids);
    }
}
use std::{
    collections::BTreeMap,
    path::PathBuf,
};

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
            ("open".to_string(), 0usize),
            ("planned".to_string(), 1usize),
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
        "planned",
        Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
    );
    let newer = ticket(
        "Newer ticket",
        "planned",
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
fn sort_candidates_prefers_more_dependees_before_newer_tickets_without_pressure()
 {
    let older = ticket(
        "Older blocker",
        "planned",
        Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
    );
    let newer = ticket(
        "Newer blocker",
        "planned",
        Utc.with_ymd_and_hms(2026, 5, 17, 12, 0, 0).unwrap(),
    );
    let dependent_one = ticket(
        "Dependent one",
        "open",
        Utc.with_ymd_and_hms(2026, 5, 18, 12, 0, 0).unwrap(),
    );
    let dependent_two = ticket(
        "Dependent two",
        "open",
        Utc.with_ymd_and_hms(2026, 5, 18, 12, 30, 0).unwrap(),
    );
    let now = Utc.with_ymd_and_hms(2026, 5, 18, 13, 0, 0).unwrap();
    let mut candidates = vec![newer.id, older.id];
    let model = build_model(
        vec![
            older.clone(),
            newer.clone(),
            dependent_one.clone(),
            dependent_two.clone(),
        ],
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
        "planned",
        Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
    );
    let newer_created = ticket(
        "Newer created but stale actionable",
        "planned",
        Utc.with_ymd_and_hms(2026, 5, 18, 12, 0, 0).unwrap(),
    );
    let recent_actionable_at =
        Utc.with_ymd_and_hms(2026, 5, 19, 12, 0, 0).unwrap();
    let stale_actionable_at =
        Utc.with_ymd_and_hms(2026, 5, 17, 12, 0, 0).unwrap();
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
        "open",
        Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
    );
    let unrelated = ticket(
        "Unrelated ready work",
        "planned",
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
    assert_eq!(
        metrics.max_affected_dependent_state.as_deref(),
        Some("in-review")
    );
    assert_eq!(metrics.dependency_state_gap, 3);
}

#[test]
fn convergence_pressure_still_beats_recently_actionable_unrelated_work() {
    let prerequisite = ticket(
        "Lagging prerequisite",
        "open",
        Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
    );
    let recently_actionable = ticket(
        "Recently actionable unrelated",
        "planned",
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
        "planned",
        Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
    );
    let direct = ticket(
        "Direct dependent",
        "open",
        Utc.with_ymd_and_hms(2026, 5, 16, 12, 30, 0).unwrap(),
    );
    let transitive = ticket(
        "Transitive dependent",
        "open",
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
        "planned",
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
        "planned",
        Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
    );
    let direct_leaf = ticket(
        "Direct frontier leaf",
        "planned",
        Utc.with_ymd_and_hms(2026, 5, 16, 12, 5, 0).unwrap(),
    );
    let nested_parent = ticket(
        "Nested parent",
        "planned",
        Utc.with_ymd_and_hms(2026, 5, 16, 12, 10, 0).unwrap(),
    );
    let nested_leaf = ticket(
        "Nested frontier leaf",
        "planned",
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
            session_id: None,
            worktree_path: None,
            branch: None,
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
        active_worktrees: Vec::new(),
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
            session_id: None,
            worktree_path: None,
            branch: None,
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
        active_worktrees: Vec::new(),
        warnings: Vec::new(),
    };

    let result =
        apply_board_filter(vec![tracked_candidate], Some(&snapshot), true);

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
        "planned",
        Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
    );
    let higher_effort = ticket(
        "Higher effort ticket",
        "planned",
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
        "planned",
        Utc.with_ymd_and_hms(2026, 5, 16, 12, 5, 0).unwrap(),
    );
    let blocked_parent = ticket(
        "Blocked parent",
        "planned",
        Utc.with_ymd_and_hms(2026, 5, 16, 12, 10, 0).unwrap(),
    );
    let external_blocker = ticket(
        "External blocker",
        "planned",
        Utc.with_ymd_and_hms(2026, 5, 16, 12, 15, 0).unwrap(),
    );
    let grandchild = ticket(
        "Grandchild",
        "open",
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
    assert_eq!(
        model.unlock_frontier_leaf_ids(root.id),
        tree.frontier_leaf_ids
    );
    assert_eq!(model.blocker_frontier_leaf_ids(root.id), vec![root.id]);
}

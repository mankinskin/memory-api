use std::collections::{
    HashMap,
    HashSet,
    VecDeque,
};

use serde_json::{
    Value,
    json,
};
use ticket_api::{
    BoardEntryStatus,
    BoardSnapshot,
    model::edge::EdgeRecord,
    storage::{
        indexed::IndexedTicket,
        store::TicketStore,
        ticket_fs::TicketFs,
    },
};
use uuid::Uuid;

use crate::cli::{
    CliRunError,
    NextArgs,
    UnblockedByArgs,
};
use crate::cli::commands::resolve_uuid_prefix;

const DONE_STATES: &[&str] = &["done", "cancelled"];
const PAUSED_STATES: &[&str] = &["on-hold"];

struct NextScope {
    root: Value,
    reachable_dependents: usize,
    blocked_dependents: usize,
    remaining_blockers: HashSet<Uuid>,
}

pub(super) fn run(
    args: NextArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let board_snap = store.board_show(None).ok();
    let all_tickets = store.list(None, None, None)?;
    let all_edges = store.list_all_edges()?;
    let mut done_ids = done_ticket_ids(&all_tickets);
    let root_id = args
        .root
        .as_deref()
        .map(|root| resolve_uuid_prefix(root, store))
        .transpose()?;
    if let Some(root_id) = root_id {
        done_ids.insert(root_id);
    }
    let blockers = unresolved_blockers(&all_edges, &done_ids);
    let next_scope = root_id.map(|root_id| {
        build_next_scope(root_id, &all_tickets, &all_edges, &blockers)
    });
    let tickets = filtered_tickets(all_tickets, args.filter.as_deref());
    let state_index = build_state_index(store);

    let mut candidates = candidate_tickets(
        &tickets,
        &blockers,
        next_scope.as_ref().map(|scope| &scope.remaining_blockers),
    );
    let priority_map = read_priorities(&candidates);
    let dependee_count = dependee_counts(&all_edges);
    sort_candidates(
        &mut candidates,
        &state_index,
        &priority_map,
        &dependee_count,
    );

    let excluded_by_board =
        excluded_by_board(board_snap.as_ref(), &candidates, args.no_board);
    let candidates =
        filter_board_candidates(candidates, board_snap.as_ref(), args.no_board);
    let limited_candidates = limit_candidates(candidates, args.limit);
    let dependency_count = dependency_counts(&all_edges);

    let mut payload = json!({
        "command": "next",
        "status": "ok",
        "count": limited_candidates.len(),
        "items": build_items(
            &limited_candidates,
            &priority_map,
            &dependency_count,
            &dependee_count,
        ),
        "excluded_by_board": excluded_by_board,
        "warnings": warnings(board_snap.as_ref()),
    });

    if let Some(scope) = next_scope {
        let obj = payload
            .as_object_mut()
            .expect("next payload should be a JSON object");
        obj.insert("root".to_string(), scope.root);
        obj.insert(
            "reachable_dependents".to_string(),
            json!(scope.reachable_dependents),
        );
        obj.insert(
            "blocked_dependents".to_string(),
            json!(scope.blocked_dependents),
        );
        obj.insert(
            "remaining_blocker_count".to_string(),
            json!(scope.remaining_blockers.len()),
        );
    }

    Ok(payload)
}

pub(super) fn run_unblocked_by(
    args: UnblockedByArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let root_id = resolve_uuid_prefix(&args.id, store)?;
    let tickets = store.list(None, None, None)?;
    let all_edges = store.list_all_edges()?;
    let mut satisfied_ids = done_ticket_ids(&tickets);
    satisfied_ids.insert(root_id);
    let blockers = unresolved_blockers(&all_edges, &satisfied_ids);
    let state_index = build_state_index(store);
    let dependent_ids = reverse_dependents(root_id, &all_edges);

    let mut candidates =
        candidate_tickets(&tickets, &blockers, Some(&dependent_ids));
    let mut still_blocked = eligible_tickets_in_scope(&tickets, Some(&dependent_ids))
        .into_iter()
        .filter(|ticket| {
            blockers
                .get(&ticket.id)
                .is_some_and(|remaining| !remaining.is_empty())
        })
        .collect::<Vec<_>>();

    let combined = candidates
        .iter()
        .chain(still_blocked.iter())
        .copied()
        .collect::<Vec<_>>();
    let priority_map = read_priorities(&combined);
    let dependee_count = dependee_counts(&all_edges);
    sort_candidates(
        &mut candidates,
        &state_index,
        &priority_map,
        &dependee_count,
    );
    sort_candidates(
        &mut still_blocked,
        &state_index,
        &priority_map,
        &dependee_count,
    );

    let dependency_count = dependency_counts(&all_edges);

    Ok(json!({
        "command": "unblocked_by",
        "status": "ok",
        "root": root_summary(root_id, &tickets),
        "reachable_dependents": dependent_ids.len(),
        "blocked_dependents": still_blocked.len(),
        "count": candidates.len(),
        "items": build_unblocked_items(
            &candidates,
            &priority_map,
            &dependency_count,
            &dependee_count,
            &blockers,
        ),
        "still_blocked_items": build_unblocked_items(
            &still_blocked,
            &priority_map,
            &dependency_count,
            &dependee_count,
            &blockers,
        ),
    }))
}

fn root_summary(
    root_id: Uuid,
    tickets: &[IndexedTicket],
) -> Value {
    let root_ticket = tickets.iter().find(|ticket| ticket.id == root_id);

    json!({
        "id": root_id,
        "title": root_ticket.and_then(|ticket| ticket.title.clone()),
        "state": root_ticket.and_then(|ticket| ticket.state.clone()),
    })
}

fn build_next_scope(
    root_id: Uuid,
    tickets: &[IndexedTicket],
    all_edges: &[EdgeRecord],
    blockers: &HashMap<Uuid, Vec<Uuid>>,
) -> NextScope {
    let dependent_ids = reverse_dependents(root_id, all_edges);
    let blocked_dependents = dependent_ids
        .iter()
        .filter(|ticket_id| {
            blockers
                .get(ticket_id)
                .is_some_and(|remaining| !remaining.is_empty())
        })
        .count();

    NextScope {
        root: root_summary(root_id, tickets),
        reachable_dependents: dependent_ids.len(),
        blocked_dependents,
        remaining_blockers: remaining_blockers_for_dependents(
            &dependent_ids,
            blockers,
        ),
    }
}

fn filtered_tickets(
    tickets: Vec<IndexedTicket>,
    filter: Option<&str>,
) -> Vec<IndexedTicket> {
    match filter {
        Some(prefix) => tickets
            .into_iter()
            .filter(|ticket| {
                ticket.title.as_deref().unwrap_or("").starts_with(prefix)
            })
            .collect(),
        None => tickets,
    }
}

fn done_ticket_ids(tickets: &[IndexedTicket]) -> HashSet<Uuid> {
    tickets
        .iter()
        .filter(|ticket| {
            ticket
                .state
                .as_deref()
                .map(|state| DONE_STATES.contains(&state))
                .unwrap_or(false)
        })
        .map(|ticket| ticket.id)
        .collect()
}

fn unresolved_blockers(
    all_edges: &[EdgeRecord],
    done_ids: &HashSet<Uuid>,
) -> HashMap<Uuid, Vec<Uuid>> {
    let mut blockers = HashMap::new();
    for edge in all_edges {
        if edge.kind == "depends_on" && !done_ids.contains(&edge.to) {
            blockers
                .entry(edge.from)
                .or_insert_with(Vec::new)
                .push(edge.to);
        }
    }
    blockers
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

fn candidate_tickets<'a>(
    tickets: &'a [IndexedTicket],
    blockers: &HashMap<Uuid, Vec<Uuid>>,
    scope: Option<&HashSet<Uuid>>,
) -> Vec<&'a IndexedTicket> {
    eligible_tickets_in_scope(tickets, scope)
        .into_iter()
        .filter(|ticket| {
            blockers
                .get(&ticket.id)
                .map_or(true, |deps| deps.is_empty())
        })
        .collect()
}

fn eligible_tickets_in_scope<'a>(
    tickets: &'a [IndexedTicket],
    scope: Option<&HashSet<Uuid>>,
) -> Vec<&'a IndexedTicket> {
    tickets
        .iter()
        .filter(|ticket| {
            scope.map_or(true, |ids| ids.contains(&ticket.id))
        })
        .filter(|ticket| is_candidate_state(ticket))
        .collect()
}

fn is_candidate_state(ticket: &IndexedTicket) -> bool {
    ticket
        .state
        .as_deref()
        .map(|state| {
            !DONE_STATES.contains(&state) && !PAUSED_STATES.contains(&state)
        })
        .unwrap_or(true)
}

fn reverse_dependents(
    root_id: Uuid,
    all_edges: &[EdgeRecord],
) -> HashSet<Uuid> {
    let mut reverse_map: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for edge in all_edges {
        if edge.kind == "depends_on" {
            reverse_map.entry(edge.to).or_default().push(edge.from);
        }
    }

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

fn remaining_blockers_for_dependents(
    dependent_ids: &HashSet<Uuid>,
    blockers: &HashMap<Uuid, Vec<Uuid>>,
) -> HashSet<Uuid> {
    dependent_ids
        .iter()
        .flat_map(|ticket_id| blockers.get(ticket_id).into_iter().flatten())
        .copied()
        .collect()
}

fn read_priorities(candidates: &[&IndexedTicket]) -> HashMap<Uuid, String> {
    let mut priority_map = HashMap::new();
    for ticket in candidates {
        if let Ok(manifest) = TicketFs::read(&ticket.path) {
            if let Some(priority) = manifest
                .extra
                .get("priority")
                .and_then(|value| value.as_str())
            {
                priority_map.insert(ticket.id, priority.to_string());
            }
        }
    }
    priority_map
}

fn sort_candidates(
    candidates: &mut Vec<&IndexedTicket>,
    state_index: &HashMap<String, usize>,
    priority_map: &HashMap<Uuid, String>,
    dependee_count: &HashMap<Uuid, usize>,
) {
    candidates.sort_by(|left, right| {
        let left_state = left.state.as_deref().unwrap_or("");
        let right_state = right.state.as_deref().unwrap_or("");
        let left_index = state_index.get(left_state).copied().unwrap_or(0);
        let right_index = state_index.get(right_state).copied().unwrap_or(0);

        right_index
            .cmp(&left_index)
            .then_with(|| {
                let left_priority = priority_map
                    .get(&left.id)
                    .map(|value| value.as_str())
                    .unwrap_or("");
                let right_priority = priority_map
                    .get(&right.id)
                    .map(|value| value.as_str())
                    .unwrap_or("");
                priority_weight(left_priority)
                    .cmp(&priority_weight(right_priority))
            })
            .then_with(|| {
                dependee_count
                    .get(&right.id)
                    .copied()
                    .unwrap_or(0)
                    .cmp(&dependee_count.get(&left.id).copied().unwrap_or(0))
            })
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| ticket_title(left).cmp(ticket_title(right)))
            .then_with(|| left.id.cmp(&right.id))
    });
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

fn excluded_by_board(
    board_snap: Option<&BoardSnapshot>,
    candidates: &[&IndexedTicket],
    no_board: bool,
) -> Vec<Value> {
    if no_board {
        return Vec::new();
    }

    let candidate_ids: HashSet<Uuid> =
        candidates.iter().map(|ticket| ticket.id).collect();
    board_snap
        .map(|snapshot| {
            snapshot
                .entries
                .iter()
                .filter(|entry| {
                    tracked_by_board(entry.status.clone())
                        && candidate_ids.contains(&entry.ticket_id)
                })
                .map(|entry| {
                    json!({
                        "ticket_id": entry.ticket_id,
                        "agent_id": entry.agent_id,
                        "status": board_status(entry.status.clone()),
                        "intent": entry.intent,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn filter_board_candidates<'a>(
    candidates: Vec<&'a IndexedTicket>,
    board_snap: Option<&BoardSnapshot>,
    no_board: bool,
) -> Vec<&'a IndexedTicket> {
    if no_board {
        return candidates;
    }

    let board_ticket_ids = board_ticket_ids(board_snap);
    candidates
        .into_iter()
        .filter(|ticket| !board_ticket_ids.contains(&ticket.id))
        .collect()
}

fn board_ticket_ids(board_snap: Option<&BoardSnapshot>) -> HashSet<Uuid> {
    board_snap
        .map(|snapshot| {
            snapshot
                .entries
                .iter()
                .filter(|entry| tracked_by_board(entry.status.clone()))
                .map(|entry| entry.ticket_id)
                .collect()
        })
        .unwrap_or_default()
}

fn tracked_by_board(status: BoardEntryStatus) -> bool {
    status == BoardEntryStatus::Active || status == BoardEntryStatus::Stale
}

fn limit_candidates<'a>(
    mut candidates: Vec<&'a IndexedTicket>,
    limit: usize,
) -> Vec<&'a IndexedTicket> {
    candidates.truncate(limit);
    candidates
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

fn build_items(
    candidates: &[&IndexedTicket],
    priority_map: &HashMap<Uuid, String>,
    dependency_count: &HashMap<Uuid, usize>,
    dependee_count: &HashMap<Uuid, usize>,
) -> Vec<Value> {
    candidates
        .iter()
        .enumerate()
        .map(|(rank, ticket)| {
            let priority = priority_map
                .get(&ticket.id)
                .cloned()
                .unwrap_or_else(|| "none".to_string());
            json!({
                "rank": rank + 1,
                "id": ticket.id,
                "title": ticket.title,
                "state": ticket.state,
                "type": ticket.type_id,
                "priority": priority,
                "dependency_count": dependency_count.get(&ticket.id).copied().unwrap_or(0),
                "dependees": dependee_count.get(&ticket.id).copied().unwrap_or(0),
                "created_at": ticket.created_at.to_rfc3339(),
            })
        })
        .collect()
}

fn build_unblocked_items(
    candidates: &[&IndexedTicket],
    priority_map: &HashMap<Uuid, String>,
    dependency_count: &HashMap<Uuid, usize>,
    dependee_count: &HashMap<Uuid, usize>,
    blockers: &HashMap<Uuid, Vec<Uuid>>,
) -> Vec<Value> {
    candidates
        .iter()
        .enumerate()
        .map(|(rank, ticket)| {
            let priority = priority_map
                .get(&ticket.id)
                .cloned()
                .unwrap_or_else(|| "none".to_string());
            let remaining_blockers = blockers
                .get(&ticket.id)
                .cloned()
                .unwrap_or_default();
            json!({
                "rank": rank + 1,
                "id": ticket.id,
                "title": ticket.title,
                "state": ticket.state,
                "type": ticket.type_id,
                "priority": priority,
                "dependency_count": dependency_count.get(&ticket.id).copied().unwrap_or(0),
                "remaining_blocker_count": remaining_blockers.len(),
                "remaining_blockers": remaining_blockers,
                "dependees": dependee_count.get(&ticket.id).copied().unwrap_or(0),
                "created_at": ticket.created_at.to_rfc3339(),
            })
        })
        .collect()
}

fn warnings(board_snap: Option<&BoardSnapshot>) -> Vec<String> {
    let Some(snapshot) = board_snap else {
        return Vec::new();
    };

    let mut warnings = Vec::new();
    let max_wip = snapshot.config.max_wip;
    if snapshot.active_count >= max_wip {
        warnings.push(format!(
            "WIP limit reached: {}/{} active entries \u{2014} pause new work and reduce the board.",
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
            "{} stale board entr{} \u{2014} heartbeat has expired; run 'ticket board heartbeat' or 'ticket board clean'.",
            snapshot.stale_count,
            if snapshot.stale_count == 1 { "y" } else { "ies" }
        ));
    }
    warnings
}

fn board_status(status: BoardEntryStatus) -> &'static str {
    match status {
        BoardEntryStatus::Active => "active",
        BoardEntryStatus::Stale => "stale",
        BoardEntryStatus::Conflict => "conflict",
        BoardEntryStatus::Completed => "completed",
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
        created_at: chrono::DateTime<Utc>,
    ) -> IndexedTicket {
        IndexedTicket {
            id: Uuid::new_v4(),
            path: PathBuf::from(title),
            type_id: "tracker-improvement".to_string(),
            title: Some(title.to_string()),
            state: Some("ready".to_string()),
            created_at,
            updated_at: created_at,
            deleted: false,
        }
    }

    #[test]
    fn sort_candidates_prefers_newer_tickets_before_older_ones() {
        let older = ticket(
            "Older ticket",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
        );
        let newer = ticket(
            "Newer ticket",
            Utc.with_ymd_and_hms(2026, 5, 17, 12, 0, 0).unwrap(),
        );
        let mut candidates = vec![&older, &newer];
        let state_index = HashMap::from([(String::from("ready"), 1usize)]);
        let priority_map = HashMap::from([
            (older.id, String::from("high")),
            (newer.id, String::from("high")),
        ]);
        let dependee_count = HashMap::new();

        sort_candidates(
            &mut candidates,
            &state_index,
            &priority_map,
            &dependee_count,
        );

        assert_eq!(candidates[0].id, newer.id);
        assert_eq!(candidates[1].id, older.id);
    }

    #[test]
    fn sort_candidates_prefers_more_dependees_before_newer_tickets() {
        let older = ticket(
            "Older ticket",
            Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap(),
        );
        let newer = ticket(
            "Newer ticket",
            Utc.with_ymd_and_hms(2026, 5, 17, 12, 0, 0).unwrap(),
        );
        let mut candidates = vec![&newer, &older];
        let state_index = HashMap::from([(String::from("ready"), 1usize)]);
        let priority_map = HashMap::from([
            (older.id, String::from("high")),
            (newer.id, String::from("high")),
        ]);
        let dependee_count = HashMap::from([(older.id, 2usize)]);

        sort_candidates(
            &mut candidates,
            &state_index,
            &priority_map,
            &dependee_count,
        );

        assert_eq!(candidates[0].id, older.id);
        assert_eq!(candidates[1].id, newer.id);
    }

    #[test]
    fn sort_candidates_uses_title_as_last_tiebreaker() {
        let created_at = Utc.with_ymd_and_hms(2026, 5, 17, 12, 0, 0).unwrap();
        let beta = ticket("Beta ticket", created_at);
        let alpha = ticket("Alpha ticket", created_at);
        let mut candidates = vec![&beta, &alpha];
        let state_index = HashMap::from([(String::from("ready"), 1usize)]);
        let priority_map = HashMap::from([
            (alpha.id, String::from("high")),
            (beta.id, String::from("high")),
        ]);
        let dependee_count = HashMap::new();

        sort_candidates(
            &mut candidates,
            &state_index,
            &priority_map,
            &dependee_count,
        );

        assert_eq!(candidates[0].id, alpha.id);
        assert_eq!(candidates[1].id, beta.id);
    }

    #[test]
    fn reverse_dependents_collects_transitive_dependents() {
        let root = Uuid::new_v4();
        let direct = Uuid::new_v4();
        let transitive = Uuid::new_v4();
        let unrelated = Uuid::new_v4();
        let now = Utc.with_ymd_and_hms(2026, 5, 22, 10, 0, 0).unwrap();
        let edges = vec![
            EdgeRecord {
                from: direct,
                to: root,
                kind: "depends_on".to_string(),
                created_at: now,
            },
            EdgeRecord {
                from: transitive,
                to: direct,
                kind: "depends_on".to_string(),
                created_at: now,
            },
            EdgeRecord {
                from: unrelated,
                to: Uuid::new_v4(),
                kind: "depends_on".to_string(),
                created_at: now,
            },
        ];

        let dependents = reverse_dependents(root, &edges);

        assert!(dependents.contains(&direct));
        assert!(dependents.contains(&transitive));
        assert!(!dependents.contains(&unrelated));
    }
}

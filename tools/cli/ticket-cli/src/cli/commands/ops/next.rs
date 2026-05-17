use std::collections::{
    HashMap,
    HashSet,
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
};

const DONE_STATES: &[&str] = &["done", "cancelled"];
const PAUSED_STATES: &[&str] = &["on-hold"];

pub(super) fn run(
    args: NextArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let board_snap = store.board_show(None).ok();
    let tickets =
        filtered_tickets(store.list(None, None, None)?, args.filter.as_deref());
    let done_ids = done_ticket_ids(&tickets);
    let all_edges = store.list_all_edges()?;
    let blockers = unresolved_blockers(&all_edges, &done_ids);
    let state_index = build_state_index(store);

    let mut candidates = candidate_tickets(&tickets, &blockers);
    let priority_map = read_priorities(&candidates);
    sort_candidates(&mut candidates, &state_index, &priority_map);

    let excluded_by_board =
        excluded_by_board(board_snap.as_ref(), &candidates, args.no_board);
    let candidates =
        filter_board_candidates(candidates, board_snap.as_ref(), args.no_board);
    let limited_candidates = limit_candidates(candidates, args.limit);
    let dependency_count = dependency_counts(&all_edges);

    Ok(json!({
        "command": "next",
        "status": "ok",
        "count": limited_candidates.len(),
        "items": build_items(&limited_candidates, &priority_map, &dependency_count),
        "excluded_by_board": excluded_by_board,
        "warnings": warnings(board_snap.as_ref()),
    }))
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
) -> Vec<&'a IndexedTicket> {
    tickets
        .iter()
        .filter(|ticket| {
            ticket
                .state
                .as_deref()
                .map(|state| {
                    !DONE_STATES.contains(&state)
                        && !PAUSED_STATES.contains(&state)
                })
                .unwrap_or(true)
        })
        .filter(|ticket| {
            blockers
                .get(&ticket.id)
                .map_or(true, |deps| deps.is_empty())
        })
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

fn build_items(
    candidates: &[&IndexedTicket],
    priority_map: &HashMap<Uuid, String>,
    dependency_count: &HashMap<Uuid, usize>,
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

        sort_candidates(&mut candidates, &state_index, &priority_map);

        assert_eq!(candidates[0].id, newer.id);
        assert_eq!(candidates[1].id, older.id);
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

        sort_candidates(&mut candidates, &state_index, &priority_map);

        assert_eq!(candidates[0].id, alpha.id);
        assert_eq!(candidates[1].id, beta.id);
    }
}

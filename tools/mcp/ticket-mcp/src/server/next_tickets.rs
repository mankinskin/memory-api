use std::collections::{
    HashMap,
    HashSet,
};

use serde_json::Value;
use ticket_api::{
    BoardEntryStatus,
    model::edge::EdgeRecord,
    storage::{
        board::BoardSnapshot,
        indexed::IndexedTicket,
        ticket_fs::TicketFs,
    },
};

use super::{
    types::*,
    *,
};

impl TicketServer {
    pub(crate) async fn next_tickets_tool(
        &self,
        input: NextTicketsInput,
    ) -> Result<CallToolResult, McpError> {
        let limit = input.limit.unwrap_or(20).min(100);
        let filter = input.filter;
        let workspace = input.workspace;

        let (items, board_value, excluded_by_board, warnings) = self
            .with_store(|store| {
                let board_snap = store.board_show(None).ok();
                let tickets = filtered_tickets(
                    store.list(None, None, None)?,
                    filter.as_deref(),
                );
                let done_ids = done_ticket_ids(&tickets);
                let blockers =
                    unresolved_blockers(&store.list_all_edges()?, &done_ids);
                let state_index = build_state_index(store);
                let mut candidates =
                    candidate_tickets(&tickets, &done_ids, &blockers);
                let priority_map = read_priorities(&candidates);

                sort_candidates(&mut candidates, &state_index, &priority_map);
                let excluded_by_board =
                    excluded_by_board(board_snap.as_ref(), &candidates);
                filter_board_candidates(&mut candidates, board_snap.as_ref());
                candidates.truncate(limit);

                Ok((
                    ranked_items(&candidates, &priority_map),
                    board_value(board_snap.as_ref()),
                    excluded_by_board,
                    warnings(board_snap.as_ref()),
                ))
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "count": items.len(),
            "items": items,
            "board": board_value,
            "excluded_by_board": excluded_by_board,
            "warnings": warnings,
        }))
    }
}

fn filtered_tickets(
    all: Vec<IndexedTicket>,
    filter: Option<&str>,
) -> Vec<IndexedTicket> {
    match filter {
        Some(prefix) => all
            .into_iter()
            .filter(|ticket| {
                ticket.title.as_deref().unwrap_or("").starts_with(prefix)
            })
            .collect(),
        None => all,
    }
}

fn done_ticket_ids(tickets: &[IndexedTicket]) -> HashSet<Uuid> {
    tickets
        .iter()
        .filter(|ticket| {
            matches!(ticket.state.as_deref(), Some("done" | "cancelled"))
        })
        .map(|ticket| ticket.id)
        .collect()
}

fn unresolved_blockers(
    all_edges: &[EdgeRecord],
    done_ids: &HashSet<Uuid>,
) -> HashMap<Uuid, Vec<Uuid>> {
    let mut blockers: HashMap<Uuid, Vec<Uuid>> = HashMap::new();

    for edge in all_edges {
        if edge.kind == "depends_on" && !done_ids.contains(&edge.to) {
            blockers.entry(edge.from).or_default().push(edge.to);
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
    done_ids: &HashSet<Uuid>,
    blockers: &HashMap<Uuid, Vec<Uuid>>,
) -> Vec<&'a IndexedTicket> {
    tickets
        .iter()
        .filter(|ticket| !done_ids.contains(&ticket.id))
        .filter(|ticket| blockers.get(&ticket.id).is_none_or(Vec::is_empty))
        .collect()
}

fn read_priorities(candidates: &[&IndexedTicket]) -> HashMap<Uuid, String> {
    let mut priorities = HashMap::new();

    for ticket in candidates {
        if let Ok(manifest) = TicketFs::read(&ticket.path) {
            if let Some(priority) = manifest
                .extra
                .get("priority")
                .and_then(|value| value.as_str())
            {
                priorities.insert(ticket.id, priority.to_string());
            }
        }
    }

    priorities
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
                    .map(String::as_str)
                    .unwrap_or("");
                let right_priority = priority_map
                    .get(&right.id)
                    .map(String::as_str)
                    .unwrap_or("");
                priority_weight(left_priority)
                    .cmp(&priority_weight(right_priority))
            })
            .then_with(|| left.created_at.cmp(&right.created_at))
    });
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
) -> Vec<Value> {
    let Some(snapshot) = board_snap else {
        return Vec::new();
    };

    let candidate_ids: HashSet<Uuid> =
        candidates.iter().map(|ticket| ticket.id).collect();

    snapshot
        .entries
        .iter()
        .filter(|entry| {
            tracked_by_board(&entry.status)
                && candidate_ids.contains(&entry.ticket_id)
        })
        .map(|entry| {
            serde_json::json!({
                "ticket_id": entry.ticket_id.to_string(),
                "agent_id": entry.agent_id.clone(),
                "status": board_status(&entry.status),
                "intent": entry.intent.clone(),
            })
        })
        .collect()
}

fn filter_board_candidates(
    candidates: &mut Vec<&IndexedTicket>,
    board_snap: Option<&BoardSnapshot>,
) {
    let Some(snapshot) = board_snap else {
        return;
    };

    let blocked_ids: HashSet<Uuid> = snapshot
        .entries
        .iter()
        .filter(|entry| tracked_by_board(&entry.status))
        .map(|entry| entry.ticket_id)
        .collect();

    candidates.retain(|ticket| !blocked_ids.contains(&ticket.id));
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

fn ranked_items(
    candidates: &[&IndexedTicket],
    priority_map: &HashMap<Uuid, String>,
) -> Vec<Value> {
    candidates
        .iter()
        .enumerate()
        .map(|(rank, ticket)| {
            serde_json::json!({
                "rank": rank + 1,
                "id": ticket.id.to_string(),
                "title": ticket.title,
                "state": ticket.state,
                "type": ticket.type_id,
                "priority": priority_map
                    .get(&ticket.id)
                    .cloned()
                    .unwrap_or_else(|| "none".to_string()),
                "created_at": ticket.created_at.to_rfc3339(),
            })
        })
        .collect()
}

fn board_value(board_snap: Option<&BoardSnapshot>) -> Value {
    let Some(snapshot) = board_snap else {
        return Value::Null;
    };

    serde_json::json!({
        "active_count": snapshot.active_count,
        "stale_count": snapshot.stale_count,
        "max_wip": snapshot.config.max_wip,
        "wip_limit_reached": snapshot.wip_limit_reached,
        "warnings": snapshot.warnings.clone(),
    })
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
            "{} stale board entr{} \u{2014} heartbeat has expired; run board heartbeat or clean.",
            snapshot.stale_count,
            if snapshot.stale_count == 1 { "y" } else { "ies" }
        ));
    }

    warnings
}

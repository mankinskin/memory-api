use std::collections::HashSet;

use serde_json::Value;
use ticket_api::{
    BoardEntryStatus,
    storage::board::BoardSnapshot,
    workflow::WorkflowModel,
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

        let (items, excluded_by_board, warnings) = self
            .with_store(&workspace, |store| {
                let board_snap = store.board_show(None).ok();
                let tickets = store.list(None, None, None)?;
                let filtered_scope =
                    filtered_ticket_scope(&tickets, filter.as_deref());
                let model =
                    WorkflowModel::build(store, tickets, store.list_all_edges()?)?;
                let mut candidates =
                    model.actionable_candidate_ids(filtered_scope.as_ref());
                model.sort_candidate_ids(&mut candidates);
                let excluded_by_board =
                    excluded_by_board(board_snap.as_ref(), &candidates);
                filter_board_candidates(&mut candidates, board_snap.as_ref());
                candidates.truncate(limit);

                Ok((
                    ranked_items(&candidates, &model),
                    excluded_by_board,
                    warnings(board_snap.as_ref()),
                ))
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "count": items.len(),
            "items": items,
            "excluded_by_board": excluded_by_board,
            "warnings": warnings,
        }))
    }
}

fn filtered_ticket_scope(
    all: &[ticket_api::storage::indexed::IndexedTicket],
    filter: Option<&str>,
) -> Option<HashSet<Uuid>> {
    filter.map(|prefix| {
        all.iter()
            .filter(|ticket| {
                ticket.title.as_deref().unwrap_or("").starts_with(prefix)
            })
            .map(|ticket| ticket.id)
            .collect()
    })
}

fn excluded_by_board(
    board_snap: Option<&BoardSnapshot>,
    candidates: &[Uuid],
) -> Vec<Value> {
    let Some(snapshot) = board_snap else {
        return Vec::new();
    };

    let candidate_ids: HashSet<Uuid> = candidates.iter().copied().collect();

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
    candidates: &mut Vec<Uuid>,
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

    candidates.retain(|ticket_id| !blocked_ids.contains(ticket_id));
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
    candidates: &[Uuid],
    model: &WorkflowModel,
) -> Vec<Value> {
    candidates
        .iter()
        .enumerate()
        .filter_map(|(rank, ticket_id)| {
            let ticket = model.ticket(ticket_id)?;
            let metrics = model.metrics(ticket_id).cloned().unwrap_or_default();
            serde_json::json!({
                "rank": rank + 1,
                "id": ticket.id.to_string(),
                "title": ticket.title,
                "state": ticket.state,
                "type": ticket.type_id,
                "priority": model.priority_or_none(ticket_id),
                "dependees": model.dependee_count(ticket_id),
                "transitive_reverse_dependents": metrics.transitive_reverse_dependents,
                "affected_reverse_dependent_reach": metrics.affected_reverse_dependent_reach,
                "max_affected_dependent_state": metrics.max_affected_dependent_state,
                "dependency_state_gap": metrics.dependency_state_gap,
                "became_actionable_at": metrics
                    .became_actionable_at
                    .map(|timestamp| timestamp.to_rfc3339()),
                "last_blocker_progress_at": metrics
                    .last_blocker_progress_at
                    .map(|timestamp| timestamp.to_rfc3339()),
                "created_at": ticket.created_at.to_rfc3339(),
            })
            .into()
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
            "{} stale board entr{} \u{2014} heartbeat has expired; run board heartbeat or clean.",
            snapshot.stale_count,
            if snapshot.stale_count == 1 { "y" } else { "ies" }
        ));
    }

    warnings
}

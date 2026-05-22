use std::collections::HashSet;

use serde_json::{
    Value,
    json,
};
use ticket_api::{
    BoardEntryStatus,
    BoardSnapshot,
    storage::store::TicketStore,
    workflow::WorkflowModel,
};
use uuid::Uuid;

use crate::cli::{
    CliRunError,
    NextArgs,
    UnblockedByArgs,
};
use crate::cli::commands::resolve_uuid_prefix;

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
    let filtered_scope = filtered_ticket_scope(&all_tickets, args.filter.as_deref());
    let model = WorkflowModel::build(store, all_tickets, store.list_all_edges()?)?;
    let root_id = args
        .root
        .as_deref()
        .map(|root| resolve_uuid_prefix(root, store))
        .transpose()?;
    let satisfied_ids = root_id.into_iter().collect::<HashSet<_>>();
    let next_scope = root_id.map(|resolved_root_id| {
        build_next_scope(resolved_root_id, &model, &satisfied_ids)
    });
    let candidate_scope = intersect_scopes(
        filtered_scope,
        next_scope.as_ref().map(|scope| &scope.remaining_blockers),
    );
    let mut candidates = if satisfied_ids.is_empty() {
        model.actionable_candidate_ids(candidate_scope.as_ref())
    } else {
        model.actionable_candidate_ids_with_satisfied(
            candidate_scope.as_ref(),
            &satisfied_ids,
        )
    };
    model.sort_candidate_ids(&mut candidates);

    let excluded_by_board =
        excluded_by_board(board_snap.as_ref(), &candidates, args.no_board);
    let candidates =
        filter_board_candidates(candidates, board_snap.as_ref(), args.no_board);
    let limited_candidates = limit_candidates(candidates, args.limit);

    let mut payload = json!({
        "command": "next",
        "status": "ok",
        "count": limited_candidates.len(),
        "items": build_items(&limited_candidates, &model),
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
    let model = WorkflowModel::build(
        store,
        store.list(None, None, None)?,
        store.list_all_edges()?,
    )?;
    let dependent_ids = model.reverse_dependents(root_id);
    let satisfied_ids = HashSet::from([root_id]);

    let mut candidates = model.actionable_candidate_ids_with_satisfied(
        Some(&dependent_ids),
        &satisfied_ids,
    );
    let mut still_blocked = model
        .eligible_candidate_ids(Some(&dependent_ids))
        .into_iter()
        .filter(|ticket_id| {
            !model
                .unresolved_dependencies_excluding(ticket_id, &satisfied_ids)
                .is_empty()
        })
        .collect::<Vec<_>>();
    model.sort_candidate_ids(&mut candidates);
    model.sort_candidate_ids(&mut still_blocked);

    Ok(json!({
        "command": "unblocked_by",
        "status": "ok",
        "root": root_summary(root_id, &model),
        "reachable_dependents": dependent_ids.len(),
        "blocked_dependents": still_blocked.len(),
        "count": candidates.len(),
        "items": build_unblocked_items(&candidates, &model, &satisfied_ids),
        "still_blocked_items": build_unblocked_items(
            &still_blocked,
            &model,
            &satisfied_ids,
        ),
    }))
}

fn root_summary(
    root_id: Uuid,
    model: &WorkflowModel,
) -> Value {
    let root_ticket = model.ticket(&root_id);

    json!({
        "id": root_id,
        "title": root_ticket.and_then(|ticket| ticket.title.clone()),
        "state": root_ticket.and_then(|ticket| ticket.state.clone()),
    })
}

fn build_next_scope(
    root_id: Uuid,
    model: &WorkflowModel,
    satisfied_ids: &HashSet<Uuid>,
) -> NextScope {
    let dependent_ids = model.reverse_dependents(root_id);
    let blocked_dependents = dependent_ids
        .iter()
        .filter(|ticket_id| {
            !model
                .unresolved_dependencies_excluding(ticket_id, satisfied_ids)
                .is_empty()
        })
        .count();

    NextScope {
        root: root_summary(root_id, model),
        reachable_dependents: dependent_ids.len(),
        blocked_dependents,
        remaining_blockers: model.remaining_blockers_for_dependents_with_satisfied(
            &dependent_ids,
            satisfied_ids,
        ),
    }
}

fn filtered_ticket_scope(
    tickets: &[ticket_api::storage::indexed::IndexedTicket],
    filter: Option<&str>,
) -> Option<HashSet<Uuid>> {
    filter.map(|prefix| {
        tickets
            .iter()
            .filter(|ticket| {
                ticket.title.as_deref().unwrap_or("").starts_with(prefix)
            })
            .map(|ticket| ticket.id)
            .collect()
    })
}

fn intersect_scopes(
    primary: Option<HashSet<Uuid>>,
    secondary: Option<&HashSet<Uuid>>,
) -> Option<HashSet<Uuid>> {
    match (primary, secondary) {
        (Some(primary), Some(secondary)) => Some(
            primary
                .into_iter()
                .filter(|ticket_id| secondary.contains(ticket_id))
                .collect(),
        ),
        (Some(primary), None) => Some(primary),
        (None, Some(secondary)) => Some(secondary.iter().copied().collect()),
        (None, None) => None,
    }
}

fn excluded_by_board(
    board_snap: Option<&BoardSnapshot>,
    candidates: &[Uuid],
    no_board: bool,
) -> Vec<Value> {
    if no_board {
        return Vec::new();
    }

    let candidate_ids: HashSet<Uuid> = candidates.iter().copied().collect();
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
    candidates: Vec<Uuid>,
    board_snap: Option<&BoardSnapshot>,
    no_board: bool,
) -> Vec<Uuid> {
    if no_board {
        return candidates;
    }

    let board_ticket_ids = board_ticket_ids(board_snap);
    candidates
        .into_iter()
        .filter(|ticket_id| !board_ticket_ids.contains(ticket_id))
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
    mut candidates: Vec<Uuid>,
    limit: usize,
) -> Vec<Uuid> {
    candidates.truncate(limit);
    candidates
}

fn build_items(
    candidates: &[Uuid],
    model: &WorkflowModel,
) -> Vec<Value> {
    candidates
        .iter()
        .enumerate()
        .filter_map(|(rank, ticket_id)| {
            let ticket = model.ticket(ticket_id)?;
            let metrics = model.metrics(ticket_id).cloned().unwrap_or_default();
            json!({
                "rank": rank + 1,
                "id": ticket.id,
                "title": ticket.title,
                "state": ticket.state,
                "type": ticket.type_id,
                "priority": model.priority_or_none(ticket_id),
                "dependency_count": model.dependency_count(ticket_id),
                "dependees": model.dependee_count(ticket_id),
                "transitive_reverse_dependents": metrics.transitive_reverse_dependents,
                "affected_reverse_dependent_reach": metrics.affected_reverse_dependent_reach,
                "max_affected_dependent_state": metrics.max_affected_dependent_state,
                "dependency_state_gap": metrics.dependency_state_gap,
                "created_at": ticket.created_at.to_rfc3339(),
            })
            .into()
        })
        .collect()
}

fn build_unblocked_items(
    candidates: &[Uuid],
    model: &WorkflowModel,
    satisfied_ids: &HashSet<Uuid>,
) -> Vec<Value> {
    candidates
        .iter()
        .enumerate()
        .filter_map(|(rank, ticket_id)| {
            let ticket = model.ticket(ticket_id)?;
            let metrics = model.metrics(ticket_id).cloned().unwrap_or_default();
            let remaining_blockers =
                model.unresolved_dependencies_excluding(ticket_id, satisfied_ids);
            json!({
                "rank": rank + 1,
                "id": ticket.id,
                "title": ticket.title,
                "state": ticket.state,
                "type": ticket.type_id,
                "priority": model.priority_or_none(ticket_id),
                "dependency_count": model.dependency_count(ticket_id),
                "remaining_blocker_count": remaining_blockers.len(),
                "remaining_blockers": remaining_blockers,
                "dependees": model.dependee_count(ticket_id),
                "transitive_reverse_dependents": metrics.transitive_reverse_dependents,
                "affected_reverse_dependent_reach": metrics.affected_reverse_dependent_reach,
                "max_affected_dependent_state": metrics.max_affected_dependent_state,
                "dependency_state_gap": metrics.dependency_state_gap,
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

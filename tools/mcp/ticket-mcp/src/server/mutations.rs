use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;
use ticket_api::model::{
    edge::EdgeRecord,
    ticket::TicketManifest,
};

use super::{
    types::*,
    *,
};

impl TicketServer {
    pub(crate) async fn update_ticket_tool(
        &self,
        input: UpdateTicketInput,
    ) -> Result<CallToolResult, McpError> {
        if input.undo {
            return self.undo_ticket_update(input).await;
        }

        let workspace = input.workspace;
        let id_str = input.id;
        let transition_states = input.transition_states;
        let to_state = input.to_state;
        let patch = parse_field_patch(input.fields, input.field_map)?;
        let description = input.description;
        let author = input.author;
        let changed_fields = patch.clone();
        let state_transition_requested = to_state.clone();
        let description_updated = description.is_some();
        let (manifest, path, previous_state) = self
            .with_store_ext(&workspace.clone(), move |store| {
                let id = Self::resolve_uuid_with(store, &id_str)?;
                let previous_state = store
                    .get_indexed(&id)
                    .map_err(Self::store_err)?
                    .and_then(|ticket| ticket.state);
                let manifest = store
                    .update(
                        &id,
                        patch,
                        Some(transition_states.as_slice()),
                        to_state.as_deref(),
                        description.as_deref(),
                        author.as_deref(),
                    )
                    .map_err(Self::store_err)?;
                let path = indexed_ticket_path(store, &id)?;
                Ok((manifest, path, previous_state))
            })
            .await?;

        let mut response = serde_json::Map::from_iter([
            ("status".to_string(), Value::String("ok".to_string())),
            ("id".to_string(), Value::String(manifest.id.to_string())),
        ]);
        if let Some(path) = path {
            response.insert("path".to_string(), Value::String(path));
        }
        if !changed_fields.is_empty() {
            response.insert(
                "changed_fields".to_string(),
                Value::Object(changed_fields.into_iter().collect()),
            );
        }
        if let Some(to_state) = state_transition_requested {
            response.insert(
                "state_transition".to_string(),
                serde_json::json!({
                    "from": previous_state,
                    "to": to_state,
                }),
            );
        }
        if description_updated {
            response.insert("description_updated".to_string(), Value::Bool(true));
        }

        Self::json_result(&Value::Object(response))
    }

    pub(crate) async fn close_ticket_tool(
        &self,
        input: CloseTicketInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let id_str = input.id;
        let author = input.author;
        let target_state = input.to_state.clone();
        let (manifest, path) = self
            .with_store_ext(&workspace.clone(), move |store| {
                let id = Self::resolve_uuid_with(store, &id_str)?;
                store
                    .close(&id, &input.to_state, author.as_deref())
                    .map_err(Self::store_err)
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "id": manifest.id.to_string(),
            "target_state": target_state,
            "traversed_states": path,
        }))
    }

    pub(crate) async fn cancel_ticket_tool(
        &self,
        input: CancelTicketInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let id_str = input.id;
        let author = input.author;
        let (manifest, path) = self
            .with_store_ext(&workspace.clone(), move |store| {
                let id = Self::resolve_uuid_with(store, &id_str)?;
                store
                    .close(&id, "cancelled", author.as_deref())
                    .map_err(Self::store_err)
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "id": manifest.id.to_string(),
            "traversed_states": path,
        }))
    }

    pub(crate) async fn create_ticket_tool(
        &self,
        input: CreateTicketInput,
    ) -> Result<CallToolResult, McpError> {
        let extra = parse_field_patch(Some(input.fields.clone()), None)?;
        let workspace = ticket_api::workspace::validate_explicit_workspace_selector(
            Some(&input.workspace),
        )
        .map_err(|err| McpError::invalid_params(err.to_string(), None))?
        .to_string();
        let type_id = input.type_id;
        let title = input.title;
        let state = input.state;
        let description = input.description;
        let (ticket_id, manifest, path) = self
            .with_store_ext(&workspace.clone(), move |store| {
                let id = store
                    .create(
                        None,
                        &type_id,
                        title.as_deref(),
                        state.as_deref(),
                        extra,
                        None,
                        description.as_deref(),
                    )
                    .map_err(Self::store_err)?;
                let manifest = store.get(&id).map_err(Self::store_err)?;
                let path = indexed_ticket_path(store, &id)?;
                Ok((id, manifest, path))
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "id": ticket_id.to_string(),
            "ticket": detail_from_manifest(manifest, path),
        }))
    }

    pub(crate) async fn delete_ticket_tool(
        &self,
        input: DeleteTicketInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let id_str = input.id;
        let id = self
            .with_store_ext(&workspace.clone(), move |store| {
                let id = Self::resolve_uuid_with(store, &id_str)?;
                store.delete(&id).map_err(Self::store_err)?;
                Ok(id)
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "id": id.to_string(),
        }))
    }

    pub(crate) async fn add_edge_tool(
        &self,
        input: AddEdgeInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let from_str = input.from;
        let to_str = input.to;
        let kind = input.kind;

        self.with_store_ext(&workspace.clone(), move |store| {
            let from = Self::resolve_uuid_with(store, &from_str)?;
            let to = Self::resolve_uuid_with(store, &to_str)?;
            let edge = EdgeRecord {
                from,
                to,
                kind: kind.clone(),
                created_at: chrono::Utc::now(),
            };
            store.add_edge(edge).map_err(Self::store_err)?;
            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "status": "ok",
                "edge": EdgeItem {
                    from: from.to_string(),
                    to: to.to_string(),
                    kind,
                },
            }))
        })
        .await
    }

    pub(crate) async fn remove_edge_tool(
        &self,
        input: RemoveEdgeInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let from_str = input.from;
        let to_str = input.to;
        let kind = input.kind;

        self.with_store_ext(&workspace.clone(), move |store| {
            let from = Self::resolve_uuid_with(store, &from_str)?;
            let to = Self::resolve_uuid_with(store, &to_str)?;
            let edge = EdgeRecord {
                from,
                to,
                kind: kind.clone(),
                created_at: chrono::Utc::now(),
            };
            store.remove_edge(edge).map_err(Self::store_err)?;
            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "status": "ok",
                "removed": EdgeItem {
                    from: from.to_string(),
                    to: to.to_string(),
                    kind,
                },
            }))
        })
        .await
    }

    pub(crate) async fn move_preflight_tool(
        &self,
        input: MovePreflightInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let id_str = input.id;
        let target_workspace = normalize_workspace_root(&input.to_workspace_root);
        let plan = self
            .with_store_ext(&workspace.clone(), move |store| {
                let id = Self::resolve_uuid_with(store, &id_str)?;
                let report = store
                    .plan_move_preflight(&id, &target_workspace)
                    .map_err(Self::store_err)?;
                Ok((id, report))
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "mode": "preflight",
            "id": plan.0.to_string(),
            "plan": move_plan_json(&plan.1),
            "recovery": move_recovery_json(),
        }))
    }

    pub(crate) async fn move_apply_tool(
        &self,
        input: MoveApplyInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let id_str = input.id;
        let target_workspace = normalize_workspace_root(&input.to_workspace_root);
        let (id, report, outcome) = self
            .with_store_ext(&workspace.clone(), move |store| {
                let id = Self::resolve_uuid_with(store, &id_str)?;
                let report = store
                    .plan_move_preflight(&id, &target_workspace)
                    .map_err(Self::store_err)?;
                if !report.supported() {
                    let blockers = serde_json::to_string(&report.blockers)
                        .unwrap_or_else(|_| "[]".to_string());
                    return Err(McpError::invalid_params(
                        format!(
                            "move preflight blocked; run move_preflight for details. blockers={blockers}"
                        ),
                        None,
                    ));
                }
                let outcome = store
                    .execute_move_with_journal(&report)
                    .map_err(Self::store_err)?;
                Ok((id, report, outcome))
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "mode": "apply",
            "id": id.to_string(),
            "plan": move_plan_json(&report),
            "outcome": move_outcome_json(&outcome),
            "recovery": move_recovery_json(),
        }))
    }

    pub(crate) async fn move_resume_tool(
        &self,
        input: MoveJournalInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let journal_id = input.id.parse::<uuid::Uuid>().map_err(|error| {
            McpError::invalid_params(
                format!("invalid move journal id '{}': {error}", input.id),
                None,
            )
        })?;

        let outcome = self
            .with_store_ext(&workspace.clone(), move |store| {
                store
                    .resume_move_with_journal(journal_id)
                    .map_err(Self::store_err)
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "mode": "resume",
            "outcome": move_outcome_json(&outcome),
            "recovery": move_recovery_json(),
        }))
    }

    pub(crate) async fn move_rollback_tool(
        &self,
        input: MoveJournalInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let journal_id = input.id.parse::<uuid::Uuid>().map_err(|error| {
            McpError::invalid_params(
                format!("invalid move journal id '{}': {error}", input.id),
                None,
            )
        })?;

        let outcome = self
            .with_store_ext(&workspace.clone(), move |store| {
                store
                    .rollback_move_with_journal(journal_id)
                    .map_err(Self::store_err)
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "mode": "rollback",
            "outcome": move_outcome_json(&outcome),
            "recovery": move_recovery_json(),
        }))
    }

    async fn undo_ticket_update(
        &self,
        input: UpdateTicketInput,
    ) -> Result<CallToolResult, McpError> {
        let has_fields = input
            .fields
            .as_ref()
            .is_some_and(|fields| !fields.is_empty());
        let has_field_map = input
            .field_map
            .as_ref()
            .is_some_and(|fields| !fields.is_empty());
        if input.to_state.is_some()
            || !input.transition_states.is_empty()
            || has_fields
            || has_field_map
        {
            return Err(McpError::invalid_params(
                "undo cannot be combined with to_state, transition_states, fields, or field_map",
                None,
            ));
        }

        let workspace = input.workspace;
        let id_str = input.id;
        let (previous_rev, new_rev, updated, path) = self
            .with_store_ext(&workspace.clone(), move |store| {
                let id = Self::resolve_uuid_with(store, &id_str)?;
                let revisions =
                    store.get_history(&id).map_err(Self::store_err)?;
                if revisions.len() < 2 {
                    return Err(Self::store_err(
                        ticket_api::error::StorageError::Database(
                            "cannot undo: not enough history revisions".into(),
                        ),
                    ));
                }
                let previous = &revisions[revisions.len() - 2];
                let new_rev = store
                    .apply_revert(&id, previous.fields.clone(), None)
                    .map_err(Self::store_err)?;
                let updated = store.get(&id).map_err(Self::store_err)?;
                let path = indexed_ticket_path(store, &id)?;
                Ok((previous.rev, new_rev, updated, path))
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "undo": true,
            "reverted_to": previous_rev,
            "new_rev": new_rev,
            "ticket": detail_from_manifest(updated, path),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_ticket_tool_rejects_default_workspace_alias() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let server = TicketServer::new(tmp.path().to_path_buf());

        let result = server
            .create_ticket_tool(CreateTicketInput {
                workspace: "default".to_string(),
                type_id: "tracker-improvement".to_string(),
                title: Some("Wrong workspace".to_string()),
                state: None,
                fields: vec![],
                description: None,
            })
            .await;

        assert!(result.is_err());
    }
}

fn move_plan_json(report: &ticket_api::storage::move_planner::MovePreflightReport) -> Value {
    serde_json::json!({
        "supported": report.supported(),
        "source_workspace_root": normalize_display_path(&report.source_workspace_root),
        "target_workspace_root": normalize_display_path(&report.target_workspace_root),
        "source_store_root": normalize_display_path(&report.source_store_root),
        "target_store_root": normalize_display_path(&report.target_store_root),
        "source_ticket_path": normalize_display_path(&report.source_entity_path),
        "destination_ticket_path": normalize_display_path(&report.destination_entity_path),
        "path_reference_files": report.path_reference_files,
        "reference_visibility": report.reference_visibility,
        "active_board_entries": report.active_board_entries,
        "historical_board_entries": report.historical_board_entries,
        "active_leases": report.active_leases,
        "blockers": report.blockers,
        "captured_at": report.captured_at,
    })
}

fn move_outcome_json(outcome: &ticket_api::storage::move_execution::MoveExecutionOutcome) -> Value {
    serde_json::json!({
        "resumed": outcome.resumed,
        "rolled_back": outcome.rolled_back,
        "journal": {
            "id": outcome.journal.id,
            "ticket_id": outcome.journal.entity_id,
            "phase": outcome.journal.phase,
            "steps": outcome.journal.steps,
            "rollback_steps": outcome.journal.rollback_steps,
            "failure": outcome.journal.failure,
            "next_recovery_step": outcome.journal.next_recovery_step,
            "rewritten_path_files": outcome.journal.rewritten_path_files,
            "manual_followups": outcome.journal.manual_followups,
            "migrated_board_entries": outcome.journal.migrated_board_entries,
            "created_at": outcome.journal.created_at,
            "updated_at": outcome.journal.updated_at,
        }
    })
}

fn move_recovery_json() -> Value {
    serde_json::json!({
        "resume": "move_resume { workspace, id: <journal-uuid> }",
        "rollback": "move_rollback { workspace, id: <journal-uuid> }",
    })
}

fn normalize_workspace_root(value: &str) -> PathBuf {
    ticket_api::workspace::canonicalize_workspace_root(&PathBuf::from(value))
}

fn normalize_display_path(path: &std::path::Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    raw.strip_prefix("//?/").unwrap_or(&raw).to_string()
}

fn parse_field_patch(
    fields: Option<Vec<String>>,
    field_map: Option<BTreeMap<String, Value>>,
) -> Result<BTreeMap<String, Value>, McpError> {
    let mut patch = field_map.unwrap_or_default();

    for raw in fields.unwrap_or_default() {
        let (key, value) = raw.split_once('=').ok_or_else(|| {
            McpError::invalid_params(
                format!("invalid field format '{raw}', expected key=value"),
                None,
            )
        })?;
        patch.insert(
            key.trim().to_string(),
            Value::String(value.trim().to_string()),
        );
    }

    Ok(patch)
}

fn indexed_ticket_path(
    store: &ticket_api::storage::store::TicketStore,
    id: &uuid::Uuid,
) -> Result<Option<String>, McpError> {
    Ok(store
        .get_indexed(id)
        .map_err(TicketServer::store_err)?
        .map(|ticket| ticket.path.display().to_string()))
}

fn detail_from_manifest(
    manifest: TicketManifest,
    path: Option<String>,
) -> TicketDetail {
    TicketDetail {
        id: manifest.id.to_string(),
        path,
        created_at: manifest.created_at,
        fields: manifest.extra,
    }
}

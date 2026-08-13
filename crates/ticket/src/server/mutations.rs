use serde_json::Value;
use ticket_api::model::edge::EdgeRecord;
use uuid::Uuid;

use super::{
    types::*,
    *,
};

#[path = "mutations/helpers.rs"]
mod mutations_helpers;
use mutations_helpers::{
    detail_from_manifest,
    indexed_ticket_path,
    move_outcome_json,
    move_plan_json,
    move_recovery_json,
    normalize_workspace_root,
    parse_field_patch,
    resolve_edge_for_remove,
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
        // The wire `description` + `description_mode` pair is decoded into
        // `input.description_update` at deserialization time by
        // `UpdateTicketInput`'s `TryFrom<UpdateTicketInputWire>` (AC5 of
        // ticket 3d952036); no runtime decode happens here.
        let description_update = input.description_update;
        let (description, description_mode) = description_update.as_parts();
        let description = description.map(str::to_string);
        let author = input.author;
        let single_hop = input.single_hop;
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
                    .update_with_options(
                        &id,
                        patch,
                        Some(transition_states.as_slice()),
                        to_state.as_deref(),
                        description.as_deref(),
                        description_mode,
                        author.as_deref(),
                        single_hop,
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
            response
                .insert("description_updated".to_string(), Value::Bool(true));
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
        let workspace =
            ticket_api::workspace::validate_explicit_workspace_selector(Some(
                &input.workspace,
            ))
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
            let edge =
                resolve_edge_for_remove(&from_str, &to_str, &kind, store)?;
            store.remove_edge(edge.clone()).map_err(Self::store_err)?;
            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "status": "ok",
                "removed": EdgeItem {
                    from: edge.from.to_string(),
                    to: edge.to.to_string(),
                    kind,
                },
            }))
        })
        .await
    }

    pub(crate) async fn prune_dangling_edges_tool(
        &self,
        input: PruneDanglingEdgesInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let root_str = input.root;
        let all = input.all;
        let kind = input.kind;
        let strategy = input.strategy;
        let reason = input.reason;

        self.with_store_ext(&workspace.clone(), move |store| {
            let root = if all {
                None
            } else {
                let raw = root_str.as_deref().ok_or_else(|| {
                    McpError::invalid_params(
                        "root is required when all=false".to_string(),
                        None,
                    )
                })?;
                Some(Self::resolve_uuid_with(store, raw)?)
            };

            let mut candidates = Vec::new();
            for edge in store.list_all_edges().map_err(Self::store_err)? {
                if edge.kind != kind {
                    continue;
                }
                if let Some(root_id) = root {
                    if edge.from != root_id {
                        continue;
                    }
                }
                let target_exists = store
                    .get_indexed(&edge.to)
                    .map_err(Self::store_err)?
                    .is_some();
                if !target_exists {
                    candidates.push(edge);
                }
            }

            let mut removed = 0usize;
            if strategy.mutates() {
                for edge in &candidates {
                    store.remove_edge(edge.clone()).map_err(Self::store_err)?;
                    removed += 1;
                }
            }

            let edges: Vec<EdgeItem> = candidates
                .iter()
                .map(|edge| EdgeItem {
                    from: edge.from.to_string(),
                    to: edge.to.to_string(),
                    kind: edge.kind.clone(),
                })
                .collect();

            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "status": "ok",
                "scope": {
                    "all": all,
                    "root": root.map(|id| id.to_string()),
                },
                "kind": kind,
                "strategy": strategy.as_str(),
                "mutated": strategy.mutates(),
                "candidate_count": edges.len(),
                "removed_count": removed,
                "reason": reason,
                "edges": edges,
            }))
        })
        .await
    }

    pub(crate) async fn move_preflight_tool(
        &self,
        input: MovePreflightInput,
    ) -> Result<CallToolResult, McpError> {
        let tool_request_id = Uuid::new_v4();
        let workspace = input.workspace;
        let id_str = input.id;
        let target_workspace =
            normalize_workspace_root(&input.to_workspace_root)?;
        let span = tracing::info_span!(
            target: "ticket_mcp::transport",
            "ticket_mcp_move_preflight",
            tool_request_id = %tool_request_id,
            workspace = %workspace,
            ticket_ref = %id_str,
            target_workspace_root = %target_workspace.display(),
        );
        let plan = self
            .with_store_ext(&workspace.clone(), move |store| {
                let id = Self::resolve_uuid_with(store, &id_str)?;
                let report = store
                    .plan_move_preflight(&id, &target_workspace)
                    .map_err(Self::store_err)?;
                Ok((id, report))
            })
            .await?;

        tracing::info!(
            target: "ticket_mcp::transport",
            parent: &span,
            tool_request_id = %tool_request_id,
            workspace = %workspace,
            ticket_id = %plan.0,
            supported = plan.1.supported(),
            blockers = plan.1.blockers.len(),
            "ticket_mcp_move_preflight_complete"
        );

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "mode": "preflight",
            "id": plan.0.to_string(),
            "plan": move_plan_json(&plan.1)?,
            "recovery": move_recovery_json(),
        }))
    }

    pub(crate) async fn move_apply_tool(
        &self,
        input: MoveApplyInput,
    ) -> Result<CallToolResult, McpError> {
        let tool_request_id = Uuid::new_v4();
        let workspace = input.workspace;
        let id_str = input.id;
        let target_workspace =
            normalize_workspace_root(&input.to_workspace_root)?;
        let span = tracing::info_span!(
            target: "ticket_mcp::transport",
            "ticket_mcp_move_apply",
            tool_request_id = %tool_request_id,
            workspace = %workspace,
            ticket_ref = %id_str,
            target_workspace_root = %target_workspace.display(),
            journal_id = tracing::field::Empty,
        );
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

        span.record("journal_id", outcome.journal.id.to_string());
        tracing::info!(
            target: "ticket_mcp::transport",
            parent: &span,
            tool_request_id = %tool_request_id,
            workspace = %workspace,
            ticket_id = %id,
            journal_id = %outcome.journal.id,
            phase = ?outcome.journal.phase,
            "ticket_mcp_move_apply_complete"
        );

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "mode": "apply",
            "id": id.to_string(),
            "plan": move_plan_json(&report)?,
            "outcome": move_outcome_json(&outcome),
            "recovery": move_recovery_json(),
        }))
    }

    pub(crate) async fn move_resume_tool(
        &self,
        input: MoveJournalInput,
    ) -> Result<CallToolResult, McpError> {
        let tool_request_id = Uuid::new_v4();
        let workspace = input.workspace;
        let journal_id = input.id.parse::<uuid::Uuid>().map_err(|error| {
            McpError::invalid_params(
                format!("invalid move journal id '{}': {error}", input.id),
                None,
            )
        })?;
        let span = tracing::info_span!(
            target: "ticket_mcp::transport",
            "ticket_mcp_move_resume",
            tool_request_id = %tool_request_id,
            workspace = %workspace,
            journal_id = %journal_id,
        );

        let outcome = self
            .with_store_ext(&workspace.clone(), move |store| {
                store
                    .resume_move_with_journal(journal_id)
                    .map_err(Self::store_err)
            })
            .await?;

        tracing::info!(
            target: "ticket_mcp::transport",
            parent: &span,
            tool_request_id = %tool_request_id,
            workspace = %workspace,
            journal_id = %outcome.journal.id,
            phase = ?outcome.journal.phase,
            resumed = outcome.resumed,
            "ticket_mcp_move_resume_complete"
        );

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
        let tool_request_id = Uuid::new_v4();
        let workspace = input.workspace;
        let journal_id = input.id.parse::<uuid::Uuid>().map_err(|error| {
            McpError::invalid_params(
                format!("invalid move journal id '{}': {error}", input.id),
                None,
            )
        })?;
        let span = tracing::info_span!(
            target: "ticket_mcp::transport",
            "ticket_mcp_move_rollback",
            tool_request_id = %tool_request_id,
            workspace = %workspace,
            journal_id = %journal_id,
        );

        let outcome = self
            .with_store_ext(&workspace.clone(), move |store| {
                store
                    .rollback_move_with_journal(journal_id)
                    .map_err(Self::store_err)
            })
            .await?;

        tracing::info!(
            target: "ticket_mcp::transport",
            parent: &span,
            tool_request_id = %tool_request_id,
            workspace = %workspace,
            journal_id = %outcome.journal.id,
            phase = ?outcome.journal.phase,
            rolled_back = outcome.rolled_back,
            "ticket_mcp_move_rollback_complete"
        );

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
                let mut revert_fields = previous.fields.clone();
                if let Some(desc_val) = revisions[revisions.len() - 1]
                    .fields
                    .get(ticket_api::storage::DESCRIPTION_HISTORY_KEY)
                {
                    revert_fields.insert(
                        ticket_api::storage::DESCRIPTION_HISTORY_KEY
                            .to_string(),
                        desc_val.clone(),
                    );
                }
                let new_rev = store
                    .apply_revert(&id, revert_fields, None)
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
#[path = "mutations/tests.rs"]
mod tests;

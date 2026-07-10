use std::{
    collections::BTreeMap,
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    error::StorageError,
    model::ticket::TicketManifest,
    storage::ticket_fs::{
        HistoryRevision,
        TicketFs,
    },
};

use super::TicketStore;

impl TicketStore {
    pub fn delete(
        &self,
        id: &Uuid,
    ) -> Result<(), StorageError> {
        let indexed =
            self.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;
        fs::remove_dir_all(&indexed.path).map_err(StorageError::Io)?;
        self.index.remove_ticket(id)?;
        self.index.remove_workflow_facts(id)?;
        self.with_search_repair(|| self.search.remove(id))?;

        if let Some(hook) = self.hook() {
            hook.ticket_delete(*id);
        }
        self.board_reconcile(id, false);

        Ok(())
    }

    pub fn force_restore(
        &self,
        id: &Uuid,
        saved_extra: BTreeMap<String, Value>,
        saved_state: Option<String>,
    ) -> Result<(), StorageError> {
        let indexed = match self.get_indexed(id)? {
            Some(ticket) => ticket,
            None => return Ok(()),
        };
        TicketFs::update(&indexed.path, &saved_extra, saved_state.as_deref())?;

        let previous_state = indexed.state.clone();
        let mut refreshed = indexed;
        refreshed.state = saved_state.clone();
        if let Some(title) = saved_extra.get("title").and_then(Value::as_str) {
            refreshed.title = Some(title.to_string());
        }
        self.index.insert_ticket(&refreshed)?;
        let body = TicketFs::read_description(&refreshed.path);
        let created_at_str = refreshed.created_at.to_rfc3339();
        let effort_str = saved_extra.get("effort").and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        });
        self.with_search_repair(|| {
            self.search.upsert(
                id,
                refreshed.title.as_deref(),
                body.as_deref(),
                refreshed.state.as_deref(),
                Some(refreshed.type_id.as_str()),
                Some(&created_at_str),
                effort_str.as_deref(),
            )
        })?;
        let state_progressed = self.state_rank_for_type(
            &refreshed.type_id,
            refreshed.state.as_deref(),
        ) > self
            .state_rank_for_type(&refreshed.type_id, previous_state.as_deref());
        self.refresh_workflow_facts_for_roots(
            &[*id],
            state_progressed,
            Utc::now(),
        )?;
        Ok(())
    }

    pub fn get_history(
        &self,
        id: &Uuid,
    ) -> Result<Vec<HistoryRevision>, StorageError> {
        let indexed =
            self.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;
        TicketFs::read_history(&indexed.path)
    }

    pub fn apply_revert(
        &self,
        id: &Uuid,
        fields: BTreeMap<String, Value>,
        author: Option<&str>,
    ) -> Result<u64, StorageError> {
        let indexed =
            self.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;

        let target_state = fields
            .get("state")
            .and_then(Value::as_str)
            .map(str::to_string);
        let mut patch = fields.clone();
        patch.remove("state");

        TicketFs::update(&indexed.path, &patch, target_state.as_deref())?;

        let previous_state = indexed.state.clone();
        let mut refreshed = indexed;
        refreshed.state = target_state.clone();
        if let Some(title) = patch.get("title").and_then(Value::as_str) {
            refreshed.title = Some(title.to_string());
        }
        self.index.insert_ticket(&refreshed)?;
        let body = TicketFs::read_description(&refreshed.path);
        let created_at_str = refreshed.created_at.to_rfc3339();
        let effort_str = patch.get("effort").and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        });
        self.with_search_repair(|| {
            self.search.upsert(
                id,
                refreshed.title.as_deref(),
                body.as_deref(),
                refreshed.state.as_deref(),
                Some(refreshed.type_id.as_str()),
                Some(&created_at_str),
                effort_str.as_deref(),
            )
        })?;
        let state_progressed = self.state_rank_for_type(
            &refreshed.type_id,
            refreshed.state.as_deref(),
        ) > self
            .state_rank_for_type(&refreshed.type_id, previous_state.as_deref());
        self.refresh_workflow_facts_for_roots(
            &[*id],
            state_progressed,
            Utc::now(),
        )?;

        let updated_manifest = TicketFs::read(&refreshed.path)?;
        let new_rev = TicketFs::append_history(
            &refreshed.path,
            updated_manifest.extra,
            author.map(str::to_string),
        )?;

        self.board_reconcile(id, true);

        Ok(new_rev)
    }

    pub fn close(
        &self,
        id: &Uuid,
        target_state: &str,
        author: Option<&str>,
    ) -> Result<(TicketManifest, Vec<String>), StorageError> {
        let indexed =
            self.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;

        let current_state = indexed.state.as_deref().unwrap_or("new");
        if current_state == target_state {
            let manifest = TicketFs::read(&indexed.path)?;
            self.board_reconcile(id, false);
            return Ok((manifest, vec![]));
        }

        let schema =
            self.schema_registry.get(&indexed.type_id).ok_or_else(|| {
                StorageError::Other(format!(
                    "no schema for type '{}'",
                    indexed.type_id
                ))
            })?;
        let path =
            schema
                .find_path(current_state, target_state)
                .ok_or_else(|| {
                    StorageError::Other(format!(
                        "no path from '{}' to '{}'",
                        current_state, target_state
                    ))
                })?;

        let empty_patch = BTreeMap::new();
        let (final_state, transition_states) = path
            .split_last()
            .expect("close path always contains at least one target state");

        let last_manifest = self.update(
            id,
            empty_patch,
            Some(transition_states),
            Some(final_state.as_str()),
            None,
            author,
        )?;

        Ok((last_manifest, path))
    }

    pub fn attach(
        &self,
        id: &Uuid,
        source_path: &Path,
        asset_name: Option<&str>,
    ) -> Result<PathBuf, StorageError> {
        let indexed =
            self.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;

        let file_name = asset_name.map(String::from).unwrap_or_else(|| {
            source_path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "attachment".to_string())
        });

        let assets_dir = indexed.path.join("assets");
        std::fs::create_dir_all(&assets_dir).map_err(|error| {
            StorageError::Other(format!("create assets dir: {error}"))
        })?;

        let destination = assets_dir.join(&file_name);
        std::fs::copy(source_path, &destination).map_err(|error| {
            StorageError::Other(format!("copy asset: {error}"))
        })?;

        let mut event = BTreeMap::new();
        event.insert("_event".to_string(), Value::String("attach".to_string()));
        event.insert("asset".to_string(), Value::String(file_name));
        let _ = TicketFs::append_history(&indexed.path, event, None);

        Ok(destination)
    }

    pub fn list_assets(
        &self,
        id: &Uuid,
    ) -> Result<Vec<String>, StorageError> {
        let indexed =
            self.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;

        let assets_dir = indexed.path.join("assets");
        if !assets_dir.exists() {
            return Ok(vec![]);
        }

        let mut names = Vec::new();
        for entry in std::fs::read_dir(&assets_dir).map_err(|error| {
            StorageError::Other(format!("read assets dir: {error}"))
        })? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_string());
            }
        }
        names.sort();
        Ok(names)
    }
}

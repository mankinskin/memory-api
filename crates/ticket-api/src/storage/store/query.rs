use crate::{
    error::StorageError,
    model::{
        edge::EdgeRecord,
        query::parse_query,
    },
    storage::{
        indexed::IndexedTicket,
        search::SearchResult,
        ticket_fs::TicketFs,
    },
};
use chrono::Utc;
use uuid::Uuid;

use super::TicketStore;

impl TicketStore {
    pub fn list(
        &self,
        state_filter: Option<&str>,
        type_filter: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<IndexedTicket>, StorageError> {
        let filtered = self
            .normalize_indexed_tickets(self.index.list_tickets(false)?)
            .into_iter()
            .filter(|ticket| matches_filters(ticket, state_filter, type_filter))
            .take(limit.unwrap_or(usize::MAX))
            .collect();
        Ok(filtered)
    }

    pub fn list_extended(
        &self,
        state_filter: Option<&str>,
        type_filter: Option<&str>,
        limit: Option<usize>,
        include_deleted: bool,
        field_filters: &[(String, String)],
    ) -> Result<Vec<IndexedTicket>, StorageError> {
        let needs_manifest_check = !field_filters.is_empty();
        let filtered = self
            .normalize_indexed_tickets(
                self.index.list_tickets(include_deleted)?,
            )
            .into_iter()
            .filter(|ticket| matches_filters(ticket, state_filter, type_filter))
            .filter(|ticket| {
                matches_field_filters(
                    ticket,
                    field_filters,
                    needs_manifest_check,
                )
            })
            .take(limit.unwrap_or(usize::MAX))
            .collect();
        Ok(filtered)
    }

    pub fn search_tickets(
        &self,
        query_expr: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError> {
        let expression =
            parse_query(query_expr).map_err(StorageError::QueryParse)?;
        self.search.search(&expression, limit)
    }

    pub fn edges_from(
        &self,
        id: &Uuid,
    ) -> Result<Vec<EdgeRecord>, StorageError> {
        self.index.edges_from(id)
    }

    pub fn list_all_edges(&self) -> Result<Vec<EdgeRecord>, StorageError> {
        self.index.list_all_edges()
    }

    pub fn count_tickets(&self) -> Result<usize, StorageError> {
        self.index.count_tickets()
    }

    pub fn count_edges(&self) -> Result<usize, StorageError> {
        self.index.count_edges()
    }

    pub fn add_edge(
        &self,
        edge: EdgeRecord,
    ) -> Result<(), StorageError> {
        let is_acyclic = self
            .schema_registry
            .get(crate::model::default_schema::TYPE_ID)
            .and_then(|schema| schema.edge_rules.get(&edge.kind))
            .map(|rule| rule.acyclic_enforced)
            .unwrap_or(false);

        if is_acyclic && self.index.is_reachable(&edge.to, &edge.from)? {
            return Err(StorageError::DependencyCycle);
        }

        let mut source = self
            .get_indexed(&edge.from)?
            .ok_or(StorageError::NotFound(edge.from))?;
        if source.deleted {
            return Err(StorageError::NotFound(edge.from));
        }

        let (manifest, changed) = TicketFs::update_edge_field(
            &source.path,
            &edge.kind,
            edge.to,
            true,
        )?;

        self.index.insert_edge(&edge)?;
        if changed {
            source.updated_at = Utc::now();
            self.index.insert_ticket(&source)?;
            let _ = TicketFs::append_history(
                &source.path,
                manifest.extra.clone(),
                None,
            );
        }
        if let Some(hook) = self.hook() {
            hook.edge_upsert(edge.from, edge.to, edge.kind.clone());
        }
        Ok(())
    }

    pub fn remove_edge(
        &self,
        edge: EdgeRecord,
    ) -> Result<(), StorageError> {
        let mut source = self
            .get_indexed(&edge.from)?
            .ok_or(StorageError::NotFound(edge.from))?;
        if source.deleted {
            return Err(StorageError::NotFound(edge.from));
        }

        let (manifest, changed) = TicketFs::update_edge_field(
            &source.path,
            &edge.kind,
            edge.to,
            false,
        )?;

        self.index.delete_edge(&edge)?;
        if changed {
            source.updated_at = Utc::now();
            self.index.insert_ticket(&source)?;
            let _ = TicketFs::append_history(
                &source.path,
                manifest.extra.clone(),
                None,
            );
        }
        if let Some(hook) = self.hook() {
            hook.edge_delete(edge.from, edge.to, edge.kind.clone());
        }
        Ok(())
    }
}

fn matches_filters(
    ticket: &IndexedTicket,
    state_filter: Option<&str>,
    type_filter: Option<&str>,
) -> bool {
    if let Some(state) = state_filter {
        if ticket.state.as_deref() != Some(state) {
            return false;
        }
    }
    if let Some(type_id) = type_filter {
        if ticket.type_id != type_id {
            return false;
        }
    }
    true
}

fn matches_field_filters(
    ticket: &IndexedTicket,
    field_filters: &[(String, String)],
    needs_manifest_check: bool,
) -> bool {
    if !needs_manifest_check {
        return true;
    }

    let manifest = match crate::storage::ticket_fs::TicketFs::read(&ticket.path)
    {
        Ok(manifest) => manifest,
        Err(_) => return false,
    };
    field_filters.iter().all(|(key, value)| {
        manifest
            .extra
            .get(key)
            .and_then(|field| field.as_str())
            .unwrap_or("")
            == value
    })
}

use crate::{
    error::StorageError,
    model::{
        edge::EdgeRecord,
        query::parse_query,
    },
    storage::{
        indexed::IndexedTicket,
        search::{
            SearchResult,
            TantivySearchIndex,
        },
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
        let mut filtered: Vec<IndexedTicket> = self
            .normalize_indexed_tickets(self.index.list_tickets()?)
            .into_iter()
            .filter(|ticket| matches_filters(ticket, state_filter, type_filter))
            .collect();
        sort_tickets_by_effort(&mut filtered);
        filtered.truncate(limit.unwrap_or(usize::MAX));
        Ok(filtered)
    }

    pub fn list_extended(
        &self,
        state_filter: Option<&str>,
        type_filter: Option<&str>,
        limit: Option<usize>,
        field_filters: &[(String, String)],
    ) -> Result<Vec<IndexedTicket>, StorageError> {
        let needs_manifest_check = !field_filters.is_empty();
        let mut filtered: Vec<IndexedTicket> = self
            .normalize_indexed_tickets(
                self.index.list_tickets()?,
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
            .collect();
        sort_tickets_by_effort(&mut filtered);
        filtered.truncate(limit.unwrap_or(usize::MAX));
        Ok(filtered)
    }

    pub fn search_tickets(
        &self,
        query_expr: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError> {
        let expression =
            parse_query(query_expr).map_err(StorageError::QueryParse)?;
        // Proactively ensure the search index is valid and complete before the
        // read, repopulating from the on-disk tickets if it is empty or partial.
        self.ensure_search_complete()?;

        match self.search.search(&expression, limit) {
            Ok(results) => Ok(results),
            // Deep segment-content corruption keeps `meta.json` valid, so it
            // passes the cheap structural and completeness checks and only
            // surfaces on read. Rebuild from the on-disk tickets and retry once.
            Err(error)
                if TantivySearchIndex::is_rebuildable_read_failure(&error) =>
            {
                self.search.reset_dir()?;
                self.scan(true)?;
                self.search.search(&expression, limit)
            },
            Err(error) => Err(error),
        }
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
        if edge.kind == "depends_on" && changed {
            self.refresh_workflow_facts_for_roots(&[edge.from], false, source.updated_at)?;
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
        if edge.kind == "depends_on" && changed {
            self.refresh_workflow_facts_for_roots(&[edge.from], true, source.updated_at)?;
        }
        Ok(())
    }
}

fn sort_tickets_by_effort(tickets: &mut [IndexedTicket]) {
    tickets.sort_by(|left, right| {
        ticket_effort(left)
            .unwrap_or(u64::MAX)
            .cmp(&ticket_effort(right).unwrap_or(u64::MAX))
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| {
                left.title
                    .as_deref()
                    .unwrap_or("")
                    .cmp(right.title.as_deref().unwrap_or(""))
            })
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn ticket_effort(ticket: &IndexedTicket) -> Option<u64> {
    TicketFs::read(&ticket.path)
        .ok()
        .and_then(|manifest| {
            manifest
                .extra
                .get("effort")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        })
        .as_deref()
        .and_then(crate::workflow::parse_effort)
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

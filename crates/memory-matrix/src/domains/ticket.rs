use std::collections::BTreeMap;

use serde_json::Value;

use crate::matrix::{
    CellResult,
    DomainOps,
    MatrixCtx,
    pass,
};

pub(crate) struct TicketDomain;

impl TicketDomain {
    fn open_strict(
        ctx: &MatrixCtx
    ) -> Result<ticket_api::storage::TicketStore, String> {
        let root = ctx.store_root(".ticket");
        if !root.exists() {
            return Err(format!(
                "ticket store root is missing at {}",
                root.display()
            ));
        }
        ticket_api::storage::TicketStore::open(&root)
            .map_err(|err| err.to_string())
    }

    fn open_or_init(
        ctx: &MatrixCtx
    ) -> Result<ticket_api::storage::TicketStore, String> {
        ticket_api::storage::TicketStore::open_or_init(
            &ctx.store_root(".ticket"),
        )
        .map_err(|err| err.to_string())
    }

    fn create_ticket(
        store: &ticket_api::storage::TicketStore,
        title: &str,
    ) -> Result<uuid::Uuid, String> {
        store
            .create(
                None,
                "tracker-improvement",
                Some(title),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .map_err(|err| err.to_string())
    }

    fn ensure_search_seed(
        store: &ticket_api::storage::TicketStore
    ) -> Result<(), String> {
        store.scan(true).map_err(|err| err.to_string())?;
        let hits = store
            .search_tickets("matrixsearchtoken", 10)
            .map_err(|err| err.to_string())?;
        if hits.is_empty() {
            let _ = Self::create_ticket(
                store,
                "matrixsearchtoken seeded representative ticket",
            )?;
            store.scan(true).map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    fn ensure_representative_volume(
        store: &ticket_api::storage::TicketStore
    ) -> Result<(), String> {
        store.scan(true).map_err(|err| err.to_string())?;
        let mut results = store
            .search_tickets("Representative fixture ticket", 50)
            .map_err(|err| err.to_string())?;
        if results.len() >= 10 {
            return Ok(());
        }

        let needed = 10usize.saturating_sub(results.len());
        for idx in 0..needed {
            let title = format!(
                "Representative fixture ticket seeded {}",
                idx + 1
            );
            let _ = Self::create_ticket(store, &title)?;
        }
        store.scan(true).map_err(|err| err.to_string())?;
        results = store
            .search_tickets("Representative fixture ticket", 50)
            .map_err(|err| err.to_string())?;
        if results.len() < 10 {
            return Err(format!(
                "ticket scan indexed too few representative tickets: {}",
                results.len()
            ));
        }
        Ok(())
    }
}

impl DomainOps for TicketDomain {
    fn domain(&self) -> &'static str {
        "ticket"
    }

    fn create(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let store = Self::open_or_init(ctx)?;
        let id = store
            .create(
                None,
                "tracker-improvement",
                Some("matrix create"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .map_err(|err| err.to_string())?;
        store.get(&id).map_err(|err| err.to_string())?;
        pass()
    }

    fn get(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let store = Self::open_strict(ctx)?;
        store.scan(true).map_err(|err| err.to_string())?;
        let seeded =
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001")
                .unwrap();
        let manifest = match store.get(&seeded) {
            Ok(manifest) => manifest,
            Err(_) => {
                let id = Self::create_ticket(&store, "Root fixture ticket")?;
                store.get(&id).map_err(|err| err.to_string())?
            }
        };
        match manifest.extra.get("title").and_then(Value::as_str) {
            Some("Root fixture ticket") => pass(),
            other => Err(format!("unexpected seeded ticket title: {other:?}")),
        }
    }

    fn search(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let store = Self::open_strict(ctx)?;
        Self::ensure_search_seed(&store)?;
        let results = store
            .search_tickets("matrixsearchtoken", 10)
            .map_err(|err| err.to_string())?;
        if results.is_empty() {
            return Err("search returned no hit for indexed token".to_string());
        }
        pass()
    }

    fn update(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let store = Self::open_strict(ctx)?;
        let id = store
            .create(
                None,
                "tracker-improvement",
                Some("matrix update"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .map_err(|err| err.to_string())?;
        let manifest = store
            .update(&id, BTreeMap::new(), None, Some("ready"), None, None)
            .map_err(|err| err.to_string())?;
        match manifest.extra.get("state").and_then(Value::as_str) {
            Some("ready") => pass(),
            other => Err(format!("update did not transition state: {other:?}")),
        }
    }

    fn delete(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let store = Self::open_strict(ctx)?;
        let id = store
            .create(
                None,
                "tracker-improvement",
                Some("matrix delete"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .map_err(|err| err.to_string())?;
        store.delete(&id).map_err(|err| err.to_string())?;
        if store.get(&id).is_ok() {
            return Err("ticket still readable after delete".to_string());
        }
        pass()
    }

    fn scan(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let store = Self::open_strict(ctx)?;
        Self::ensure_representative_volume(&store)?;
        pass()
    }
}

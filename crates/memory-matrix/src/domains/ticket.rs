use std::collections::BTreeMap;

use serde_json::Value;

use crate::matrix::{pass, CellResult, DomainOps, MatrixCtx};

pub(crate) struct TicketDomain;

impl TicketDomain {
    fn open(ctx: &MatrixCtx) -> Result<ticket_api::storage::TicketStore, String> {
        ticket_api::storage::TicketStore::open_or_init(&ctx.store_root(".ticket"))
            .map_err(|err| err.to_string())
    }
}

impl DomainOps for TicketDomain {
    fn domain(&self) -> &'static str {
        "ticket"
    }

    fn create(&self, ctx: &MatrixCtx) -> CellResult {
        let store = Self::open(ctx)?;
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

    fn get(&self, ctx: &MatrixCtx) -> CellResult {
        let store = Self::open(ctx)?;
        store.scan(true).map_err(|err| err.to_string())?;
        let seeded =
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let manifest = store.get(&seeded).map_err(|err| err.to_string())?;
        match manifest.extra.get("title").and_then(Value::as_str) {
            Some("Root fixture ticket") => pass(),
            other => Err(format!("unexpected seeded ticket title: {other:?}")),
        }
    }

    fn search(&self, ctx: &MatrixCtx) -> CellResult {
        let store = Self::open(ctx)?;
        store
            .create(
                None,
                "tracker-improvement",
                Some("matrixsearchtoken ticket"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .map_err(|err| err.to_string())?;
        store.scan(true).map_err(|err| err.to_string())?;
        let results = store
            .search_tickets("matrixsearchtoken", 10)
            .map_err(|err| err.to_string())?;
        if results.is_empty() {
            return Err("search returned no hit for indexed token".to_string());
        }
        pass()
    }

    fn update(&self, ctx: &MatrixCtx) -> CellResult {
        let store = Self::open(ctx)?;
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

    fn delete(&self, ctx: &MatrixCtx) -> CellResult {
        let store = Self::open(ctx)?;
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

    fn scan(&self, ctx: &MatrixCtx) -> CellResult {
        let store = Self::open(ctx)?;
        store.scan(true).map_err(|err| err.to_string())?;
        pass()
    }
}

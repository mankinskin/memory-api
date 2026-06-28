use std::collections::BTreeMap;

use serde_json::json;

use crate::matrix::{pass, CellResult, DomainOps, MatrixCtx};

pub(crate) struct SpecDomain;

impl SpecDomain {
    fn open(ctx: &MatrixCtx) -> Result<spec_api::SpecStore, String> {
        spec_api::SpecStore::open_or_init(&ctx.store_root(".spec"))
            .map_err(|err| err.to_string())
    }

    fn new_manifest(slug: &str, title: &str) -> spec_api::SpecManifest {
        spec_api::SpecManifest::new(slug, title, "matrix")
    }
}

impl DomainOps for SpecDomain {
    fn domain(&self) -> &'static str {
        "spec"
    }

    fn create(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        let manifest = Self::new_manifest("matrix/create", "Matrix Create");
        store
            .create(&manifest, "matrix body", None)
            .map_err(|err| err.to_string())?;
        store
            .get(&manifest.id().to_string())
            .map_err(|err| err.to_string())?;
        pass()
    }

    fn get(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        let manifest = Self::new_manifest("matrix/get", "Matrix Get");
        store
            .create(&manifest, "matrix body", None)
            .map_err(|err| err.to_string())?;
        store.scan(true).map_err(|err| err.to_string())?;
        let fetched = store.get("matrix/get").map_err(|err| err.to_string())?;
        match fetched.title() {
            Some("Matrix Get") => pass(),
            other => Err(format!("unexpected spec title: {other:?}")),
        }
    }

    fn search(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        let manifest = Self::new_manifest("matrix/search", "Matrixspectoken Spec");
        store
            .create(&manifest, "matrix body", None)
            .map_err(|err| err.to_string())?;
        store.scan(true).map_err(|err| err.to_string())?;
        let results = store
            .entity_store()
            .search("Matrixspectoken", 10)
            .map_err(|err| err.to_string())?;
        if results.is_empty() {
            return Err("spec search returned no hit for indexed token".to_string());
        }
        pass()
    }

    fn update(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        let manifest = Self::new_manifest("matrix/update", "Matrix Update");
        store
            .create(&manifest, "matrix body", None)
            .map_err(|err| err.to_string())?;
        let mut patch = BTreeMap::new();
        patch.insert("scope".to_string(), json!("internal"));
        let updated = store
            .update("matrix/update", patch, None)
            .map_err(|err| err.to_string())?;
        match updated.scope() {
            Some("internal") => pass(),
            other => Err(format!("spec update did not apply patch: {other:?}")),
        }
    }

    fn delete(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        let manifest = Self::new_manifest("matrix/delete", "Matrix Delete");
        store
            .create(&manifest, "matrix body", None)
            .map_err(|err| err.to_string())?;
        store.delete("matrix/delete").map_err(|err| err.to_string())?;
        if store.get("matrix/delete").is_ok() {
            return Err("spec still readable after delete".to_string());
        }
        pass()
    }

    fn scan(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        store.scan(true).map_err(|err| err.to_string())?;
        pass()
    }
}

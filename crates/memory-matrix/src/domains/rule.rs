use crate::matrix::{
    CellResult,
    DomainOps,
    MatrixCtx,
    pass,
};

pub(crate) struct RuleDomain;

impl RuleDomain {
    fn open_strict(ctx: &MatrixCtx) -> Result<rule_api::RuleStore, String> {
        let root = ctx.store_root(".rule");
        if !root.exists() {
            return Err(format!(
                "rule store root is missing at {}",
                root.display()
            ));
        }
        rule_api::RuleStore::open(&root)
            .map_err(|err| err.to_string())
    }

    fn open_or_init(ctx: &MatrixCtx) -> Result<rule_api::RuleStore, String> {
        rule_api::RuleStore::open_or_init(&ctx.store_root(".rule"))
            .map_err(|err| err.to_string())
    }

    fn new_manifest(
        slug: &str,
        title: &str,
    ) -> rule_api::RuleManifest {
        rule_api::RuleManifest::new(slug, title, "markdown", "matrix", "body")
    }

    fn ensure_fixture_rule(
        store: &mut rule_api::RuleStore
    ) -> Result<(), String> {
        if store.get("fixture/rule-search").is_ok() {
            return Ok(());
        }
        let manifest =
            Self::new_manifest("fixture/rule-search", "Matrixruletoken Rule");
        store.create(&manifest, None).map_err(|err| err.to_string())?;
        Ok(())
    }
}

impl DomainOps for RuleDomain {
    fn domain(&self) -> &'static str {
        "rule"
    }

    fn create(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let mut store = Self::open_or_init(ctx)?;
        let manifest = Self::new_manifest("matrix/create", "Matrix Create");
        store
            .create(&manifest, None)
            .map_err(|err| err.to_string())?;
        store
            .get(&manifest.id.to_string())
            .map_err(|err| err.to_string())?;
        pass()
    }

    fn get(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let mut store = Self::open_strict(ctx)?;
        store.scan(true).map_err(|err| err.to_string())?;
        Self::ensure_fixture_rule(&mut store)?;
        let fetched = store
            .get("fixture/rule-search")
            .map_err(|err| err.to_string())?;
        match fetched.title() {
            Some("Matrixruletoken Rule") => pass(),
            other => Err(format!("unexpected rule title: {other:?}")),
        }
    }

    fn search(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let mut store = Self::open_strict(ctx)?;
        store.scan(true).map_err(|err| err.to_string())?;
        let results = store
            .search("Matrixruletoken", &rule_api::RuleFilter::default(), 10)
            .map_err(|err| err.to_string())?;
        if results.is_empty() {
            Self::ensure_fixture_rule(&mut store)?;
            store.scan(true).map_err(|err| err.to_string())?;
            let retry = store
                .search(
                    "Matrixruletoken",
                    &rule_api::RuleFilter::default(),
                    10,
                )
                .map_err(|err| err.to_string())?;
            if retry.is_empty() {
                return Err(
                    "rule search returned no hit for indexed token"
                        .to_string()
                );
            }
        }
        pass()
    }

    fn update(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let mut store = Self::open_strict(ctx)?;
        let manifest = Self::new_manifest("matrix/update", "Matrix Update");
        store
            .create(&manifest, None)
            .map_err(|err| err.to_string())?;
        store
            .update_body("matrix/update", "updated body")
            .map_err(|err| err.to_string())?;
        let fetched =
            store.get("matrix/update").map_err(|err| err.to_string())?;
        match fetched.body() {
            Some("updated body") => pass(),
            other => Err(format!("rule update_body did not apply: {other:?}")),
        }
    }

    fn delete(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let mut store = Self::open_strict(ctx)?;
        let manifest = Self::new_manifest("matrix/delete", "Matrix Delete");
        store
            .create(&manifest, None)
            .map_err(|err| err.to_string())?;
        store
            .delete("matrix/delete")
            .map_err(|err| err.to_string())?;
        if store.get("matrix/delete").is_ok() {
            return Err("rule still readable after delete".to_string());
        }
        pass()
    }

    fn scan(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let mut store = Self::open_strict(ctx)?;
        store.scan(true).map_err(|err| err.to_string())?;
        Self::ensure_fixture_rule(&mut store)?;
        store
            .get("fixture/rule-search")
            .map_err(|err| err.to_string())?;
        pass()
    }
}

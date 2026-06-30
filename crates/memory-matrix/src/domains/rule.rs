use std::process::Command;

use crate::matrix::{
    CellResult,
    DomainOps,
    MatrixCtx,
    pass,
};

pub(crate) struct RuleDomain;

impl RuleDomain {
    fn run_git(
        repo_root: &std::path::Path,
        args: &[&str],
    ) -> Result<(), String> {
        let status = Command::new("git")
            .current_dir(repo_root)
            .args(args)
            .status()
            .map_err(|err| format!("git {args:?} failed to start: {err}"))?;
        if status.success() {
            return Ok(());
        }
        Err(format!("git {args:?} failed: {status}"))
    }

    fn run_move_roundtrip() -> Result<(), String> {
        let repo = std::env::temp_dir()
            .join(format!("memory-matrix-rule-move-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&repo)
            .map_err(|err| format!("create move repo `{}`: {err}", repo.display()))?;
        Self::run_git(&repo, &["init"])?;

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).map_err(|err| {
            format!(
                "create source workspace `{}`: {err}",
                source_workspace.display()
            )
        })?;
        std::fs::create_dir_all(&target_workspace).map_err(|err| {
            format!(
                "create target workspace `{}`: {err}",
                target_workspace.display()
            )
        })?;

        let mut source_store = rule_api::RuleStore::init(&source_workspace)
            .map_err(|err| err.to_string())?;
        let _target_store = rule_api::RuleStore::init(&target_workspace)
            .map_err(|err| err.to_string())?;

        let manifest = rule_api::RuleManifest::new(
            "matrix/move",
            "Matrix Move",
            "markdown",
            "memory-matrix",
            "body",
        );
        let rule_id = source_store
            .create(&manifest, None)
            .map_err(|err| err.to_string())?;
        source_store.scan(true).map_err(|err| err.to_string())?;

        let mut plan = source_store
            .plan_move_preflight(&rule_id, &target_workspace)
            .map_err(|err| err.to_string())?;
        plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                memory_api::storage::move_kernel::MoveBlocker::PathReferenceScanUnavailable { .. }
                    | memory_api::storage::move_kernel::MoveBlocker::DirtyTrackedFiles { .. }
            )
        });

        if !plan.supported() {
            return Err(format!(
                "rule move preflight remained blocked in matrix harness: {:?}",
                plan.blockers
            ));
        }

        source_store
            .execute_move_with_journal(&plan)
            .map_err(|err| err.to_string())?;

        let src = rule_api::RuleStore::open(&source_workspace)
            .map_err(|err| err.to_string())?;
        let dst = rule_api::RuleStore::open(&target_workspace)
            .map_err(|err| err.to_string())?;
        if src
            .entity_store()
            .get_indexed(&rule_id)
            .map_err(|err| err.to_string())?
            .is_some()
        {
            return Err("rule still indexed in source workspace after move".to_string());
        }
        if dst
            .entity_store()
            .get_indexed(&rule_id)
            .map_err(|err| err.to_string())?
            .is_none()
        {
            return Err("rule missing from destination workspace after move".to_string());
        }

        Ok(())
    }

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

    fn move_op(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        Self::run_move_roundtrip()?;
        pass()
    }
}

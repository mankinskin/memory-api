use std::{
    collections::BTreeMap,
    process::Command,
};

use serde_json::json;

use crate::matrix::{
    CellResult,
    DomainOps,
    MatrixCtx,
    pass,
};

pub(crate) struct SpecDomain;

impl SpecDomain {
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
            .join(format!("memory-matrix-spec-move-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&repo).map_err(|err| {
            format!("create move repo `{}`: {err}", repo.display())
        })?;
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

        let mut source_store = spec_api::SpecStore::init(&source_workspace)
            .map_err(|err| err.to_string())?;
        let _target_store = spec_api::SpecStore::init(&target_workspace)
            .map_err(|err| err.to_string())?;

        let manifest = spec_api::SpecManifest::new(
            "matrix/move",
            "Matrix Move",
            "memory-matrix",
        );
        let spec_id = source_store
            .create(&manifest, "body", None)
            .map_err(|err| err.to_string())?;
        source_store.scan(true).map_err(|err| err.to_string())?;

        let mut plan = source_store
            .plan_move_preflight(&spec_id, &target_workspace)
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
                "spec move preflight remained blocked in matrix harness: {:?}",
                plan.blockers
            ));
        }

        source_store
            .execute_move_with_journal(&plan)
            .map_err(|err| err.to_string())?;

        let src = spec_api::SpecStore::open(&source_workspace)
            .map_err(|err| err.to_string())?;
        let dst = spec_api::SpecStore::open(&target_workspace)
            .map_err(|err| err.to_string())?;
        if src
            .entity_store()
            .get_indexed(&spec_id)
            .map_err(|err| err.to_string())?
            .is_some()
        {
            return Err(
                "spec still indexed in source workspace after move".to_string()
            );
        }
        if dst
            .entity_store()
            .get_indexed(&spec_id)
            .map_err(|err| err.to_string())?
            .is_none()
        {
            return Err("spec missing from destination workspace after move"
                .to_string());
        }

        Ok(())
    }

    fn open_strict(ctx: &MatrixCtx) -> Result<spec_api::SpecStore, String> {
        let root = ctx.store_root(".spec");
        if !root.exists() {
            return Err(format!(
                "spec store root is missing at {}",
                root.display()
            ));
        }
        spec_api::SpecStore::open(&root).map_err(|err| err.to_string())
    }

    fn open_or_init(ctx: &MatrixCtx) -> Result<spec_api::SpecStore, String> {
        spec_api::SpecStore::open_or_init(&ctx.store_root(".spec"))
            .map_err(|err| err.to_string())
    }

    fn new_manifest(
        slug: &str,
        title: &str,
    ) -> spec_api::SpecManifest {
        spec_api::SpecManifest::new(slug, title, "matrix")
    }

    fn ensure_fixture_root(
        store: &mut spec_api::SpecStore
    ) -> Result<(), String> {
        if store.get("fixture/root").is_ok() {
            return Ok(());
        }
        let manifest = Self::new_manifest("fixture/root", "Root fixture spec");
        store
            .create(&manifest, "fixture body", None)
            .map_err(|err| err.to_string())?;
        Ok(())
    }
}

impl DomainOps for SpecDomain {
    fn domain(&self) -> &'static str {
        "spec"
    }

    fn create(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let mut store = Self::open_or_init(ctx)?;
        let manifest = Self::new_manifest("matrix/create", "Matrix Create");
        store
            .create(&manifest, "matrix body", None)
            .map_err(|err| err.to_string())?;
        store
            .get(&manifest.id().to_string())
            .map_err(|err| err.to_string())?;
        pass()
    }

    fn get(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let mut store = Self::open_strict(ctx)?;
        store.scan(true).map_err(|err| err.to_string())?;
        Self::ensure_fixture_root(&mut store)?;
        let fetched =
            store.get("fixture/root").map_err(|err| err.to_string())?;
        match fetched.title() {
            Some("Root fixture spec") => pass(),
            other => Err(format!("unexpected spec title: {other:?}")),
        }
    }

    fn search(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let mut store = Self::open_strict(ctx)?;
        store.scan(true).map_err(|err| err.to_string())?;
        let results = store
            .entity_store()
            .search("Root fixture", 10)
            .map_err(|err| err.to_string())?;
        if results.is_empty() {
            Self::ensure_fixture_root(&mut store)?;
            store.scan(true).map_err(|err| err.to_string())?;
            let retry = store
                .entity_store()
                .search("Root fixture", 10)
                .map_err(|err| err.to_string())?;
            if retry.is_empty() {
                return Err(
                    "spec search returned no hit for indexed token".to_string()
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

    fn delete(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let mut store = Self::open_strict(ctx)?;
        let manifest = Self::new_manifest("matrix/delete", "Matrix Delete");
        store
            .create(&manifest, "matrix body", None)
            .map_err(|err| err.to_string())?;
        store
            .delete("matrix/delete")
            .map_err(|err| err.to_string())?;
        if store.get("matrix/delete").is_ok() {
            return Err("spec still readable after delete".to_string());
        }
        pass()
    }

    fn scan(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let mut store = Self::open_strict(ctx)?;
        store.scan(true).map_err(|err| err.to_string())?;
        Self::ensure_fixture_root(&mut store)?;
        store.get("fixture/root").map_err(|err| err.to_string())?;
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

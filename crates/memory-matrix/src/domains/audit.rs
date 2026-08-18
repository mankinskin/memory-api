use crate::matrix::{
    CellResult,
    DomainOps,
    MatrixCtx,
    blocked,
};

pub(crate) struct AuditDomain;

impl AuditDomain {
    fn open(
        ctx: &MatrixCtx
    ) -> Result<audit_api::index::RepositoryIndex, String> {
        audit_api::index::RepositoryIndex::open_or_init(&ctx.workspace_root)
            .map_err(|err| err.to_string())
    }
}

impl DomainOps for AuditDomain {
    fn domain(&self) -> &'static str {
        "audit"
    }

    fn scan(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let index = Self::open(ctx)?;
        index
            .sync_source_files(&[])
            .map_err(|err| err.to_string())?;
        crate::matrix::pass()
    }

    fn search(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let index = Self::open(ctx)?;
        index
            .sync_source_files(&[])
            .map_err(|err| err.to_string())?;
        index.indexed_files().map_err(|err| err.to_string())?;
        crate::matrix::pass()
    }

    fn create(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        blocked(
            "audit-api `record_audit_run` requires a fully populated \
             AuditMetrics snapshot produced by a complete `audit()` run; \
             not exercisable as a unit create in the matrix",
        )
    }
}

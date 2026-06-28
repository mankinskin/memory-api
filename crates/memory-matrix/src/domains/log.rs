use chrono::Utc;

use crate::matrix::{blocked, pass, CellResult, DomainOps, MatrixCtx};

pub(crate) struct LogDomain;

impl LogDomain {
    fn config(ctx: &MatrixCtx) -> log_api::LogStoreConfig {
        log_api::LogStoreConfig::new(ctx.store_root(".log"), "default")
    }

    fn capture(id: &str, detail: &str) -> log_api::ValidationLogCapture {
        log_api::ValidationLogCapture {
            id: id.to_string(),
            validation_execution_id: "vt-log-domain".to_string(),
            kind: log_api::ValidationLogKind::CombinedOutput,
            captured_at: Utc::now(),
            media_type: "text/plain".to_string(),
            locator: "memory://matrix".to_string(),
            detail: Some(detail.to_string()),
            links: log_api::ValidationLogLinks::default(),
        }
    }
}

impl DomainOps for LogDomain {
    fn domain(&self) -> &'static str {
        "log"
    }

    fn create(&self, ctx: &MatrixCtx) -> CellResult {
        let config = Self::config(ctx);
        config
            .record_capture(&Self::capture("matrix-create", "first"))
            .map_err(|err| err.to_string())?;
        config
            .get_capture("matrix-create")
            .map_err(|err| err.to_string())?;
        pass()
    }

    fn get(&self, ctx: &MatrixCtx) -> CellResult {
        let config = Self::config(ctx);
        config
            .record_capture(&Self::capture("matrix-get", "first"))
            .map_err(|err| err.to_string())?;
        let fetched = config
            .get_capture("matrix-get")
            .map_err(|err| err.to_string())?;
        if fetched.id == "matrix-get" {
            pass()
        } else {
            Err(format!("unexpected capture id: {}", fetched.id))
        }
    }

    fn search(&self, ctx: &MatrixCtx) -> CellResult {
        let config = Self::config(ctx);
        config
            .record_capture(&Self::capture("matrix-search", "first"))
            .map_err(|err| err.to_string())?;
        let captures = config
            .list_captures(&log_api::LogCaptureQuery::default())
            .map_err(|err| err.to_string())?;
        if captures.is_empty() {
            return Err("capture query returned no records".to_string());
        }
        pass()
    }

    fn update(&self, ctx: &MatrixCtx) -> CellResult {
        let config = Self::config(ctx);
        config
            .record_capture(&Self::capture("matrix-update", "first"))
            .map_err(|err| err.to_string())?;
        config
            .record_capture(&Self::capture("matrix-update", "second"))
            .map_err(|err| err.to_string())?;
        let fetched = config
            .get_capture("matrix-update")
            .map_err(|err| err.to_string())?;
        if fetched.detail.as_deref() == Some("second") {
            pass()
        } else {
            Err("re-record did not overwrite capture detail".to_string())
        }
    }

    fn delete(&self, _ctx: &MatrixCtx) -> CellResult {
        blocked("log-api exposes no delete operation for captures")
    }

    fn scan(&self, _ctx: &MatrixCtx) -> CellResult {
        blocked(
            "log-api has no scan/index reconcile; captures are listed directly \
             from disk",
        )
    }
}

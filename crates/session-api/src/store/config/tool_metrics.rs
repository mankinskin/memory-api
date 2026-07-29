use crate::{
    SessionError,
    ToolMetricsReport,
    ToolMetricsWindow,
    aggregate_with_cost,
    compute_session_summary_with_events,
    write_rollup,
    tool_metrics::{CharsPerTokenEstimator, GradedCostCalibration, SessionToolMetricsSummary},
};
use std::path::PathBuf;

impl SessionStoreConfig {
    /// Get aggregate tool metrics for sessions in this store, respecting the window.
    pub fn tool_metrics(
        &self,
        window: ToolMetricsWindow,
    ) -> Result<ToolMetricsReport, SessionError> {
        let sessions_root = self.sessions_root()?;
        if !sessions_root.exists() {
            // No sessions, return empty report
            let estimator = CharsPerTokenEstimator::default();
            let cal = GradedCostCalibration::default();
            return Ok(aggregate_with_cost(vec![], window, &estimator, Some(cal)));
        }

        let mut summaries = Vec::new();

        for entry in fs::read_dir(&sessions_root).map_err(|source| {
            SessionError::Io {
                path: sessions_root.clone(),
                source,
            }
        })? {
            let entry = entry.map_err(|source| SessionError::Io {
                path: sessions_root.clone(),
                source,
            })?;

            let file_type = entry.file_type().map_err(|source| {
                SessionError::Io {
                    path: entry.path(),
                    source,
                }
            })?;

            if !file_type.is_dir() {
                continue;
            }

            let session_id = entry.file_name().to_string_lossy().into_owned();

            // Skip directories that are not readable sessions (e.g. runtime
            // scratch folders): they must not fail the whole aggregation.
            let summary = match self
                .load_or_compute_tool_metrics_summary(&session_id)
            {
                Ok(summary) => summary,
                Err(SessionError::NotFound { .. }) => continue,
                Err(SessionError::Deserialize { .. }) => continue,
                Err(error) => return Err(error),
            };
            summaries.push(summary);
        }

        let estimator = CharsPerTokenEstimator::default();
        let cal = GradedCostCalibration::default();
        Ok(aggregate_with_cost(summaries, window, &estimator, Some(cal)))
    }

    /// Write tool metrics rollup to the canonical location.
    pub fn write_tool_metrics_rollup(
        &self,
        window: ToolMetricsWindow,
    ) -> Result<(), SessionError> {
        let report = self.tool_metrics(window)?;
        let rollup_path = self.root.join("tool-metrics-rollup.json");
        write_rollup(&rollup_path, report)
    }

    fn load_or_compute_tool_metrics_summary(
        &self,
        session_id: &str,
    ) -> Result<SessionToolMetricsSummary, SessionError> {
        let tool_metrics_path = self.tool_metrics_path(session_id)?;

        // Reuse a cached summary only when it actually holds tool data. An
        // empty cached summary predates event-based capture, so recompute it.
        if let Some(summary) =
            read_json_if_exists::<SessionToolMetricsSummary>(&tool_metrics_path)?
        {
            if !summary.is_empty() {
                return Ok(summary);
            }
        }

        // Compute from the transcript plus the captured event stream, which is
        // where tool call telemetry actually lives.
        let record = self.read_session(session_id)?;
        let paths = self.paths_for_session_id(session_id)?;
        let events: Option<crate::PersistedSessionEvents> =
            read_json_if_exists(&paths.events_path)?;
        let estimator = CharsPerTokenEstimator::default();
        let summary = compute_session_summary_with_events(
            &record,
            events
                .as_ref()
                .map(|events| events.events.as_slice())
                .unwrap_or_default(),
            &estimator,
        );

        // Persist lazily: never leave an empty sidecar behind.
        if summary.is_empty() {
            remove_file_if_exists(&tool_metrics_path)?;
        } else {
            write_json(&tool_metrics_path, &summary)?;
        }

        Ok(summary)
    }

    fn tool_metrics_path(&self, session_id: &str) -> Result<PathBuf, SessionError> {
        let paths = self.paths_for_session_id(session_id)?;
        Ok(paths.session_dir.join("tool-metrics.json"))
    }
}

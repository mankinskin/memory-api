use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::{SessionError, SessionRecord, SessionRole};

/// Trait for estimating token counts from character counts.
pub trait TokenEstimator {
    fn estimate_tokens(&self, chars: u64) -> f64;
}

/// Default token estimator using a fixed chars-per-token ratio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharsPerTokenEstimator {
    pub chars_per_token: f64,
}

impl Default for CharsPerTokenEstimator {
    fn default() -> Self {
        Self {
            chars_per_token: 4.0,
        }
    }
}

impl TokenEstimator for CharsPerTokenEstimator {
    fn estimate_tokens(&self, chars: u64) -> f64 {
        chars as f64 / self.chars_per_token
    }
}

/// Per-tool token statistics aggregated across sessions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolTokenStats {
    pub tool_name: String,
    pub call_count: u64,
    pub success_count: u64,
    pub total_output_chars: u64,
    pub mean_output_chars: f64,
    pub p50_output_chars: u64,
    pub p90_output_chars: u64,
    pub p95_output_chars: u64,
    pub max_output_chars: u64,
    pub est_mean_output_tokens: f64,
    pub est_p90_output_tokens: f64,
    pub mean_input_chars: f64,
    /// Optional graded cost (1..=scale_max) for this tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<u32>,
}

/// Aggregated tool metrics report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolMetricsReport {
    pub schema_version: u32,
    pub generated_at: DateTime<Utc>,
    pub session_count: usize,
    pub turn_count: usize,
    pub chars_per_token: f64,
    pub window: ToolMetricsWindowDescription,
    pub tools: Vec<ToolTokenStats>,
}

/// Description of the window used for aggregation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolMetricsWindowDescription {
    pub max_age_days: Option<u32>,
    pub max_sessions: Option<usize>,
    pub actual_sessions: usize,
    pub oldest_session_date: Option<DateTime<Utc>>,
    pub newest_session_date: Option<DateTime<Utc>>,
}

/// Calibration for graded cost mapping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradedCostCalibration {
    /// Maximum value of the cost scale (default 100).
    pub scale_max: u32,
    /// Token count that maps to scale_max (tunable anchor).
    /// Default 150.0 places typical heavy tools mid-high on the scale.
    pub tokens_at_max: f64,
}

impl Default for GradedCostCalibration {
    fn default() -> Self {
        Self {
            scale_max: 100,
            // TODO: provisional anchor; re-tune from the first empirical tool-metrics rollup
            tokens_at_max: 8000.0,
        }
    }
}

/// Compute graded cost from estimated tokens using linear mapping.
/// Maps [0, tokens_at_max] -> [1, scale_max], clamped.
pub fn graded_cost(est_tokens: f64, cal: &GradedCostCalibration) -> u32 {
    if est_tokens <= 0.0 {
        return 1;
    }
    let ratio = est_tokens / cal.tokens_at_max;
    let scaled = (ratio * cal.scale_max as f64).ceil() as u32;
    scaled.clamp(1, cal.scale_max)
}

/// Schema-versioned rollup containing the report plus per-tool graded costs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolMetricsRollup {
    pub schema_version: u32,
    pub report: ToolMetricsReport,
}

/// Window configuration for tool metrics aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolMetricsWindow {
    pub max_age_days: Option<u32>,
    pub max_sessions: Option<usize>,
}

impl Default for ToolMetricsWindow {
    fn default() -> Self {
        Self {
            max_age_days: Some(30),
            max_sessions: Some(100),
        }
    }
}

/// Per-session tool metrics summary (no transcript content, only sizes and counts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionToolMetricsSummary {
    pub schema_version: u32,
    pub session_id: String,
    pub captured_at: DateTime<Utc>,
    pub tools: BTreeMap<String, ToolCallSummary>,
}

/// Per-tool call summary within a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallSummary {
    pub call_count: u64,
    pub success_count: u64,
    pub output_char_sizes: Vec<u64>,
    pub input_char_sizes: Vec<u64>,
}

const TOOL_METRICS_SCHEMA_VERSION: u32 = 1;

/// Compute per-session tool metrics summary from a session record.
pub fn compute_session_summary(
    record: &SessionRecord,
    _estimator: &impl TokenEstimator,
) -> SessionToolMetricsSummary {
    let mut tools = BTreeMap::<String, ToolCallSummary>::new();

    for turn in &record.turns {
        if turn.role != SessionRole::Tool {
            continue;
        }
        let Some(tool_name) = &turn.tool_name else {
            continue;
        };

        let entry = tools.entry(tool_name.clone()).or_insert_with(|| {
            ToolCallSummary {
                call_count: 0,
                success_count: 0,
                output_char_sizes: Vec::new(),
                input_char_sizes: Vec::new(),
            }
        });

        entry.call_count += 1;

        // Check if this is a successful call (not explicitly failed)
        let is_success = turn
            .event_meta
            .as_ref()
            .and_then(|meta| meta.tool_success)
            != Some(false);

        if is_success {
            entry.success_count += 1;
            let output_chars = turn.content.chars().count() as u64;
            entry.output_char_sizes.push(output_chars);

            // Capture input size from tool_arguments_json
            if let Some(event_meta) = &turn.event_meta {
                if let Some(args_json) = &event_meta.tool_arguments_json {
                    let input_chars =
                        serde_json::to_string(args_json).unwrap_or_default().len() as u64;
                    entry.input_char_sizes.push(input_chars);
                }
            }
        }
    }

    SessionToolMetricsSummary {
        schema_version: TOOL_METRICS_SCHEMA_VERSION,
        session_id: record.session_id.clone(),
        captured_at: record.captured_at,
        tools,
    }
}

/// Aggregate tool metrics from multiple session summaries with window filtering.
pub fn aggregate(
    summaries: Vec<SessionToolMetricsSummary>,
    window: ToolMetricsWindow,
    estimator: &impl TokenEstimator,
) -> ToolMetricsReport {
    aggregate_with_cost(summaries, window, estimator, None)
}

/// Aggregate tool metrics with optional cost calibration.
pub fn aggregate_with_cost(
    summaries: Vec<SessionToolMetricsSummary>,
    window: ToolMetricsWindow,
    estimator: &impl TokenEstimator,
    cost_cal: Option<GradedCostCalibration>,
) -> ToolMetricsReport {
    let mut filtered_summaries = summaries;

    // Apply time window filter
    if let Some(max_age_days) = window.max_age_days {
        let cutoff = Utc::now() - Duration::days(max_age_days as i64);
        filtered_summaries.retain(|s| s.captured_at >= cutoff);
    }

    // Sort by captured_at descending (most recent first)
    filtered_summaries.sort_by(|a, b| b.captured_at.cmp(&a.captured_at));

    // Apply session count limit
    if let Some(max_sessions) = window.max_sessions {
        filtered_summaries.truncate(max_sessions);
    }

    let session_count = filtered_summaries.len();
    let oldest_session_date = filtered_summaries.last().map(|s| s.captured_at);
    let newest_session_date = filtered_summaries.first().map(|s| s.captured_at);

    // Aggregate tool stats
    let mut tool_data = BTreeMap::<String, ToolAggregation>::new();
    let mut total_turn_count = 0usize;

    for summary in &filtered_summaries {
        for (tool_name, call_summary) in &summary.tools {
            let entry = tool_data.entry(tool_name.clone()).or_insert_with(|| {
                ToolAggregation {
                    call_count: 0,
                    success_count: 0,
                    output_chars: Vec::new(),
                    input_chars: Vec::new(),
                }
            });

            entry.call_count += call_summary.call_count;
            entry.success_count += call_summary.success_count;
            entry.output_chars.extend(&call_summary.output_char_sizes);
            entry.input_chars.extend(&call_summary.input_char_sizes);

            total_turn_count += call_summary.call_count as usize;
        }
    }

    // Compute per-tool statistics
    let mut tools = Vec::new();
    let cal = cost_cal.unwrap_or_default();
    for (tool_name, data) in tool_data {
        let mut sorted_output = data.output_chars.clone();
        sorted_output.sort_unstable();

        let total_output_chars: u64 = sorted_output.iter().sum();
        let mean_output_chars = if sorted_output.is_empty() {
            0.0
        } else {
            total_output_chars as f64 / sorted_output.len() as f64
        };

        let p50_output_chars = percentile(&sorted_output, 50);
        let p90_output_chars = percentile(&sorted_output, 90);
        let p95_output_chars = percentile(&sorted_output, 95);
        let max_output_chars = sorted_output.last().copied().unwrap_or(0);

        let est_mean_output_tokens = estimator.estimate_tokens(mean_output_chars as u64);
        let est_p90_output_tokens = estimator.estimate_tokens(p90_output_chars);

        let mean_input_chars = if data.input_chars.is_empty() {
            0.0
        } else {
            let total_input: u64 = data.input_chars.iter().sum();
            total_input as f64 / data.input_chars.len() as f64
        };

        let cost = if cost_cal.is_some() {
            Some(graded_cost(est_p90_output_tokens, &cal))
        } else {
            None
        };

        tools.push(ToolTokenStats {
            tool_name,
            call_count: data.call_count,
            success_count: data.success_count,
            total_output_chars,
            mean_output_chars,
            p50_output_chars,
            p90_output_chars,
            p95_output_chars,
            max_output_chars,
            est_mean_output_tokens,
            est_p90_output_tokens,
            mean_input_chars,
            cost,
        });
    }

    // Sort tools by est_p90_output_tokens descending
    tools.sort_by(|a, b| {
        b.est_p90_output_tokens
            .partial_cmp(&a.est_p90_output_tokens)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.tool_name.cmp(&b.tool_name))
    });

    ToolMetricsReport {
        schema_version: TOOL_METRICS_SCHEMA_VERSION,
        generated_at: Utc::now(),
        session_count,
        turn_count: total_turn_count,
        chars_per_token: match estimator as &dyn TokenEstimator {
            e if std::ptr::eq(e as *const _, &CharsPerTokenEstimator::default() as *const _) => {
                CharsPerTokenEstimator::default().chars_per_token
            }
            _ => 4.0, // fallback
        },
        window: ToolMetricsWindowDescription {
            max_age_days: window.max_age_days,
            max_sessions: window.max_sessions,
            actual_sessions: session_count,
            oldest_session_date,
            newest_session_date,
        },
        tools,
    }
}

#[derive(Debug, Clone)]
struct ToolAggregation {
    call_count: u64,
    success_count: u64,
    output_chars: Vec<u64>,
    input_chars: Vec<u64>,
}

fn percentile(sorted_values: &[u64], p: u8) -> u64 {
    if sorted_values.is_empty() {
        return 0;
    }
    if p >= 100 {
        return *sorted_values.last().unwrap();
    }
    // Nearest rank method: rank = ceil(p/100 * n)
    let rank = (p as f64 / 100.0 * sorted_values.len() as f64).ceil() as usize;
    sorted_values[rank.saturating_sub(1)]
}

/// Write a tool metrics rollup to a file.
pub fn write_rollup(path: &Path, report: ToolMetricsReport) -> Result<(), SessionError> {
    use std::fs;
    use std::io::Write;
    
    let rollup = ToolMetricsRollup {
        schema_version: TOOL_METRICS_SCHEMA_VERSION,
        report,
    };
    
    let json = serde_json::to_string_pretty(&rollup)
        .map_err(|source| SessionError::Serialize { 
            path: path.to_path_buf(),
            source,
        })?;
    
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| SessionError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    
    let mut file = fs::File::create(path).map_err(|source| SessionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    
    file.write_all(json.as_bytes()).map_err(|source| SessionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    
    Ok(())
}

/// Aggregate tool metrics across multiple store roots.
/// Each store root is expected to contain a `sessions/` subdirectory.
/// This function is a convenience wrapper around store-level aggregation logic.
pub fn aggregate_multi_store(
    store_roots: &[std::path::PathBuf],
    window: ToolMetricsWindow,
) -> Result<ToolMetricsReport, SessionError> {
    // Import here to avoid circular dependencies
    use crate::SessionStoreConfig;
    
    let mut all_summaries = Vec::new();
    
    for store_root in store_roots {
        let config = SessionStoreConfig::new(store_root, "default");
        
        // Get summaries from this store using the same logic as tool_metrics()
        // but don't aggregate yet - just collect
        let sessions_root = store_root.join("sessions");
        
        if !sessions_root.exists() {
            continue;
        }

        for entry in std::fs::read_dir(&sessions_root).map_err(|source| {
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
            
            // Try to load cached summary
            let tool_metrics_path = entry.path().join("tool-metrics.json");
            
            if tool_metrics_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&tool_metrics_path) {
                    if let Ok(summary) = serde_json::from_str::<SessionToolMetricsSummary>(&content) {
                        all_summaries.push(summary);
                        continue;
                    }
                }
            }
            
            // If cached summary doesn't exist, compute it
            if let Ok(record) = config.read_session(&session_id) {
                let estimator = CharsPerTokenEstimator::default();
                let summary = compute_session_summary(&record, &estimator);
                all_summaries.push(summary);
            }
        }
    }

    let estimator = CharsPerTokenEstimator::default();
    let cal = GradedCostCalibration::default();
    Ok(aggregate_with_cost(all_summaries, window, &estimator, Some(cal)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SessionMetadata, SessionTurn, SessionTurnEventMeta, SESSION_SCHEMA_VERSION};
    use chrono::TimeZone;

    fn sample_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0)
            .single()
            .unwrap()
    }

    fn sample_time_offset(days: i64) -> DateTime<Utc> {
        sample_time() + Duration::days(days)
    }

    #[test]
    fn percentile_computation_is_correct() {
        let values = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        assert_eq!(percentile(&values, 50), 50);
        assert_eq!(percentile(&values, 90), 90);
        assert_eq!(percentile(&values, 95), 100);
    }

    #[test]
    fn failure_exclusion_from_size_percentiles() {
        let record = SessionRecord {
            schema_version: SESSION_SCHEMA_VERSION,
            session_id: "test-session".to_string(),
            source: "test".to_string(),
            started_at: sample_time(),
            captured_at: sample_time(),
            metadata: SessionMetadata {
                workspace_slug: "test".to_string(),
                conversation_id: None,
                agent_id: None,
                ticket_id: None,
                model: None,
                trigger: None,
                producer: None,
                copilot_version: None,
                vscode_version: None,
                protocol_version: None,
                worktree: None,
            },
            turns: vec![
                SessionTurn {
                    sequence: 1,
                    role: SessionRole::Tool,
                    content: "success output".to_string(),
                    captured_at: sample_time(),
                    tool_name: Some("test_tool".to_string()),
                    model: None,
                    event_meta: Some(SessionTurnEventMeta {
                        tool_success: Some(true),
                        tool_arguments_json: None,
                        event_id: None,
                        parent_event_id: None,
                        event_type: None,
                        turn_id: None,
                        message_id: None,
                        tool_call_id: None,
                        reasoning_text: None,
                        tool_requests_json: None,
                    }),
                },
                SessionTurn {
                    sequence: 2,
                    role: SessionRole::Tool,
                    content: "failure output".to_string(),
                    captured_at: sample_time(),
                    tool_name: Some("test_tool".to_string()),
                    model: None,
                    event_meta: Some(SessionTurnEventMeta {
                        tool_success: Some(false),
                        tool_arguments_json: None,
                        event_id: None,
                        parent_event_id: None,
                        event_type: None,
                        turn_id: None,
                        message_id: None,
                        tool_call_id: None,
                        reasoning_text: None,
                        tool_requests_json: None,
                    }),
                },
            ],
            links: Default::default(),
        };

        let estimator = CharsPerTokenEstimator::default();
        let summary = compute_session_summary(&record, &estimator);

        let tool_summary = &summary.tools["test_tool"];
        assert_eq!(tool_summary.call_count, 2);
        assert_eq!(tool_summary.success_count, 1);
        assert_eq!(tool_summary.output_char_sizes.len(), 1);
        assert_eq!(tool_summary.output_char_sizes[0], "success output".chars().count() as u64);
    }

    #[test]
    fn window_cap_filters_correctly() {
        let summaries = vec![
            SessionToolMetricsSummary {
                schema_version: TOOL_METRICS_SCHEMA_VERSION,
                session_id: "s1".to_string(),
                captured_at: sample_time_offset(-35), // 35 days ago, outside 30-day window
                tools: BTreeMap::new(),
            },
            SessionToolMetricsSummary {
                schema_version: TOOL_METRICS_SCHEMA_VERSION,
                session_id: "s2".to_string(),
                captured_at: sample_time_offset(-10), // 10 days ago, inside window
                tools: BTreeMap::new(),
            },
            SessionToolMetricsSummary {
                schema_version: TOOL_METRICS_SCHEMA_VERSION,
                session_id: "s3".to_string(),
                captured_at: sample_time_offset(-5), // 5 days ago, inside window
                tools: BTreeMap::new(),
            },
        ];

        let window = ToolMetricsWindow {
            max_age_days: Some(30),
            max_sessions: Some(100),
        };
        let estimator = CharsPerTokenEstimator::default();

        let report = aggregate(summaries, window, &estimator);
        assert_eq!(report.session_count, 2); // Only s2 and s3
    }

    #[test]
    fn window_cap_respects_session_limit() {
        let summaries: Vec<SessionToolMetricsSummary> = (0..150)
            .map(|i| SessionToolMetricsSummary {
                schema_version: TOOL_METRICS_SCHEMA_VERSION,
                session_id: format!("session-{}", i),
                captured_at: sample_time_offset(-(i as i64)),
                tools: BTreeMap::new(),
            })
            .collect();

        let window = ToolMetricsWindow {
            max_age_days: None, // No time filter, only session limit
            max_sessions: Some(100),
        };
        let estimator = CharsPerTokenEstimator::default();

        let report = aggregate(summaries, window, &estimator);
        assert_eq!(report.session_count, 100);
    }

    #[test]
    fn privacy_no_transcript_content_in_summary() {
        let record = SessionRecord {
            schema_version: SESSION_SCHEMA_VERSION,
            session_id: "test-session".to_string(),
            source: "test".to_string(),
            started_at: sample_time(),
            captured_at: sample_time(),
            metadata: SessionMetadata {
                workspace_slug: "test".to_string(),
                conversation_id: None,
                agent_id: None,
                ticket_id: None,
                model: None,
                trigger: None,
                producer: None,
                copilot_version: None,
                vscode_version: None,
                protocol_version: None,
                worktree: None,
            },
            turns: vec![SessionTurn {
                sequence: 1,
                role: SessionRole::Tool,
                content: "secret data".to_string(),
                captured_at: sample_time(),
                tool_name: Some("test_tool".to_string()),
                model: None,
                event_meta: None,
            }],
            links: Default::default(),
        };

        let estimator = CharsPerTokenEstimator::default();
        let summary = compute_session_summary(&record, &estimator);
        let serialized = serde_json::to_string(&summary).unwrap();

        assert!(!serialized.contains("secret data"));
        assert!(serialized.contains("test_tool"));
    }

    #[test]
    fn tokenizer_default_uses_4_chars_per_token() {
        let estimator = CharsPerTokenEstimator::default();
        assert_eq!(estimator.chars_per_token, 4.0);
        assert_eq!(estimator.estimate_tokens(100), 25.0);
    }

    #[test]
    fn graded_cost_linear_mapping() {
        let cal = GradedCostCalibration {
            scale_max: 100,
            tokens_at_max: 8000.0,
        };
        
        // Zero maps to floor
        assert_eq!(graded_cost(0.0, &cal), 1);
        
        // Below zero maps to floor
        assert_eq!(graded_cost(-10.0, &cal), 1);
        
        // At anchor maps to ceiling
        assert_eq!(graded_cost(8000.0, &cal), 100);
        
        // Above anchor clamps to ceiling
        assert_eq!(graded_cost(20000.0, &cal), 100);
        
        // Mid-range linear mapping
        assert_eq!(graded_cost(4000.0, &cal), 50);  // half of anchor -> half of scale
        assert_eq!(graded_cost(800.0, &cal), 10);  // 10% of anchor -> 10% of scale
        assert_eq!(graded_cost(80.0, &cal), 1);  // 1% of anchor -> 1% of scale (clamped to floor)
    }

    #[test]
    fn graded_cost_clamped_to_range() {
        let cal = GradedCostCalibration {
            scale_max: 50,
            tokens_at_max: 100.0,
        };
        
        // Floor clamp
        assert_eq!(graded_cost(0.1, &cal), 1);
        
        // Ceiling clamp
        assert_eq!(graded_cost(200.0, &cal), 50);
    }

    #[test]
    fn rollup_round_trips_through_serde() {
        let report = ToolMetricsReport {
            schema_version: TOOL_METRICS_SCHEMA_VERSION,
            generated_at: sample_time(),
            session_count: 10,
            turn_count: 100,
            chars_per_token: 4.0,
            window: ToolMetricsWindowDescription {
                max_age_days: Some(30),
                max_sessions: Some(100),
                actual_sessions: 10,
                oldest_session_date: Some(sample_time_offset(-10)),
                newest_session_date: Some(sample_time()),
            },
            tools: vec![
                ToolTokenStats {
                    tool_name: "test_tool".to_string(),
                    call_count: 10,
                    success_count: 9,
                    total_output_chars: 1000,
                    mean_output_chars: 100.0,
                    p50_output_chars: 90,
                    p90_output_chars: 150,
                    p95_output_chars: 180,
                    max_output_chars: 200,
                    est_mean_output_tokens: 25.0,
                    est_p90_output_tokens: 37.5,
                    mean_input_chars: 50.0,
                    cost: Some(25),
                },
            ],
        };

        let rollup = ToolMetricsRollup {
            schema_version: TOOL_METRICS_SCHEMA_VERSION,
            report: report.clone(),
        };

        let serialized = serde_json::to_string(&rollup).unwrap();
        let deserialized: ToolMetricsRollup = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.report.tools[0].cost, Some(25));
        assert_eq!(deserialized.report.session_count, 10);
    }
}

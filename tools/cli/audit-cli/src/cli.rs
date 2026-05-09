use std::{
    ffi::OsString,
    path::PathBuf,
};

use clap::{
    Args,
    Parser,
    Subcommand,
    ValueEnum,
};
use serde_json::{
    Value,
    json,
};

use audit_api::{
    audit::audit,
    error::AuditError,
    models::{
        AuditConfig,
        AuditReport,
        TrialStatus,
    },
    summary::{
        AuditSummaryBy,
        AuditSummaryReport,
        summarize_report,
    },
};

#[derive(Debug, Parser)]
#[command(
    name = "audit",
    about = "Repository quality audit CLI",
    version,
    arg_required_else_help = true
)]
pub struct AuditCli {
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: AuditCommand,
}

#[derive(Debug, Subcommand)]
pub enum AuditCommand {
    /// Run an audit for a repository.
    Run(AuditArgs),

    /// Summarize findings grouped by one key.
    Summary(AuditSummaryArgs),
}

#[derive(Debug, Args)]
pub struct AuditArgs {
    /// Repository root to audit.
    #[arg(default_value = ".")]
    pub repo_root: PathBuf,

    #[arg(long)]
    pub max_file_lines: Option<usize>,

    #[arg(long)]
    pub max_cyclomatic_complexity: Option<usize>,

    #[arg(long)]
    pub coverage_warn_below: Option<f64>,
}

#[derive(Debug, Args)]
pub struct AuditSummaryArgs {
    #[arg(long, value_enum)]
    pub by: SummaryByArg,

    #[command(flatten)]
    pub args: AuditArgs,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SummaryByArg {
    Crate,
    Package,
    Category,
    Severity,
    Metric,
    Path,
}

impl From<SummaryByArg> for AuditSummaryBy {
    fn from(value: SummaryByArg) -> Self {
        match value {
            SummaryByArg::Crate | SummaryByArg::Package =>
                AuditSummaryBy::Crate,
            SummaryByArg::Category => AuditSummaryBy::Category,
            SummaryByArg::Severity => AuditSummaryBy::Severity,
            SummaryByArg::Metric => AuditSummaryBy::Metric,
            SummaryByArg::Path => AuditSummaryBy::Path,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CliRunError {
    #[error("audit error: {0}")]
    Audit(#[from] AuditError),
}

pub enum CliOutput {
    Json(Value),
    Text(String),
}

pub fn parse_cli_from<I, T>(args: I) -> Result<AuditCli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    AuditCli::try_parse_from(args)
}

pub fn run(cli: AuditCli) -> Result<CliOutput, CliRunError> {
    match cli.command {
        AuditCommand::Run(args) => {
            let report = run_audit(&args)?;
            if cli.json {
                Ok(CliOutput::Json(json!(report)))
            } else {
                Ok(CliOutput::Text(render_human(&report)))
            }
        },
        AuditCommand::Summary(summary) => {
            let report = run_audit(&summary.args)?;
            let summary = summarize_report(&report, summary.by.into())?;
            if cli.json {
                Ok(CliOutput::Json(json!(summary)))
            } else {
                Ok(CliOutput::Text(render_summary_human(&summary)))
            }
        },
    }
}

fn run_audit(args: &AuditArgs) -> Result<AuditReport, CliRunError> {
    let mut config = AuditConfig::default();
    if let Some(max_file_lines) = args.max_file_lines {
        config.max_file_lines = max_file_lines;
    }
    if let Some(max_cyclomatic_complexity) = args.max_cyclomatic_complexity {
        config.max_cyclomatic_complexity = max_cyclomatic_complexity;
    }
    if let Some(coverage_warn_below) = args.coverage_warn_below {
        config.coverage_warn_below = coverage_warn_below;
    }

    Ok(audit(&args.repo_root, config)?)
}

pub fn error_output(
    message: &str,
    as_json: bool,
) -> String {
    if as_json {
        serde_json::to_string_pretty(&json!({
            "code": "invalid_request",
            "message": message,
        }))
        .unwrap_or_else(|_| {
            format!(
                "{{\"code\":\"invalid_request\",\"message\":{:?}}}",
                message
            )
        })
    } else {
        message.to_string()
    }
}

fn render_human(report: &AuditReport) -> String {
    let mut lines = Vec::new();
    lines.push("Repository Audit".to_string());
    lines.push(format!("Repo: {}", report.repo_root));
    lines.push(format!("Index: {}", report.index_database));
    lines.push(format!(
        "Sync: scanned {}, updated {}, reused {}, pruned {}",
        report.sync.scanned_files,
        report.sync.updated_files,
        report.sync.reused_files,
        report.sync.pruned_files
    ));
    lines.push(format!(
        "Files: {} source files, {} total lines",
        report.metrics.source_files, report.metrics.total_lines
    ));
    lines.push(format!(
        "File length: {} long files over {} lines (max {})",
        report.metrics.file_length.long_files,
        report.metrics.file_length.threshold,
        report.metrics.file_length.max_lines
    ));
    lines.push(format!(
        "Compiler warnings: {}",
        render_count_metric(&report.metrics.compiler_warnings)
    ));
    lines.push(format!(
        "Test success: {}",
        render_test_metric(&report.metrics.test_results)
    ));
    lines.push(format!(
        "Coverage: {}",
        render_coverage_metric(&report.metrics.coverage)
    ));
    lines.push(format!(
        "Static metrics: {} high-complexity functions over threshold {} (avg {})",
        report.metrics.static_metrics.high_complexity_functions,
        report.metrics.static_metrics.threshold,
        report
            .metrics
            .static_metrics
            .average_cyclomatic_complexity
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "n/a".to_string())
    ));

    if report.findings.is_empty() {
        lines.push("Findings: none".to_string());
    } else {
        lines.push(format!("Findings: {}", report.findings.len()));
        for finding in &report.findings {
            let mut line =
                format!("- [{:?}] {}", finding.severity, finding.summary);
            if let Some(path) = &finding.path {
                line.push_str(&format!(" ({path})"));
            }
            lines.push(line);
            for instruction in &finding.instructions {
                lines.push(format!("  fix: {instruction}"));
            }
        }
    }

    lines.join("\n")
}

fn render_summary_human(summary: &AuditSummaryReport) -> String {
    let mut lines = Vec::new();
    lines.push("Repository Audit Summary".to_string());
    lines.push(format!("Repo: {}", summary.repo_root));
    lines.push(format!("Grouped by: {}", summary.by.as_str()));
    lines.push(format!("Total findings: {}", summary.total_findings));
    lines.push(format!("Repo-wide issues: {}", summary.repo_wide_issues));

    if summary.groups.is_empty() {
        lines.push("Groups: none".to_string());
    } else {
        lines.push("Groups:".to_string());
        for group in &summary.groups {
            lines.push(format!("- {}: {}", group.key, group.issues));
        }
    }

    if !summary.unmapped_paths.is_empty() {
        lines.push("Unmapped paths:".to_string());
        for group in &summary.unmapped_paths {
            lines.push(format!("- {}: {}", group.key, group.issues));
        }
    }

    lines.join("\n")
}

fn render_count_metric(metric: &audit_api::models::CountMetric) -> String {
    match metric.status {
        TrialStatus::Collected | TrialStatus::Failed => metric
            .count
            .map(|count| count.to_string())
            .unwrap_or_else(|| "n/a".to_string()),
        TrialStatus::Unavailable | TrialStatus::NotApplicable => metric
            .details
            .clone()
            .unwrap_or_else(|| "unavailable".to_string()),
    }
}

fn render_test_metric(metric: &audit_api::models::TestSummary) -> String {
    match metric.status {
        TrialStatus::Collected | TrialStatus::Failed => format!(
            "{} passed, {} failed, {} ignored, success rate {}",
            metric.passed.unwrap_or_default(),
            metric.failed.unwrap_or_default(),
            metric.ignored.unwrap_or_default(),
            metric
                .success_rate
                .map(|value| format!("{value:.1}%"))
                .unwrap_or_else(|| "n/a".to_string())
        ),
        TrialStatus::Unavailable | TrialStatus::NotApplicable => metric
            .details
            .clone()
            .unwrap_or_else(|| "unavailable".to_string()),
    }
}

fn render_coverage_metric(
    metric: &audit_api::models::CoverageSummary
) -> String {
    match metric.status {
        TrialStatus::Collected => metric
            .line_percent
            .map(|value| format!("{value:.1}%"))
            .unwrap_or_else(|| "n/a".to_string()),
        TrialStatus::Unavailable
        | TrialStatus::NotApplicable
        | TrialStatus::Failed => metric
            .details
            .clone()
            .unwrap_or_else(|| "unavailable".to_string()),
    }
}

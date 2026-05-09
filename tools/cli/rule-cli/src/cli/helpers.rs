use std::{
    collections::BTreeMap,
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use rule_api::{
    RuleFilter,
    RuleManifest,
};
use serde_json::{
    Value,
    json,
};

use super::{
    CliRunError,
    FilterArgs,
};

pub(super) fn resolve_index_root(explicit: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        return path.to_path_buf();
    }
    if let Ok(path) = std::env::var("RULE_INDEX_ROOT") {
        return PathBuf::from(path);
    }
    if let Ok(path) = std::env::var("TICKET_INDEX_ROOT") {
        return PathBuf::from(path);
    }
    if let Some(path) =
        std::env::current_dir().ok().map(|dir| dir.join(".rule"))
    {
        if path.exists() {
            return path;
        }
    }
    if let Ok(home) =
        std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"))
    {
        return PathBuf::from(home).join(".rule-index");
    }
    PathBuf::from(".rule")
}

pub(super) fn read_body(
    inline: Option<String>,
    body_file: Option<&Path>,
) -> Result<String, CliRunError> {
    match (inline, body_file) {
        (_, Some(path)) => fs::read_to_string(path).map_err(|err| {
            CliRunError::BadRequest(format!(
                "read body file {}: {err}",
                path.display()
            ))
        }),
        (Some(body), None) => Ok(body),
        (None, None) => Ok(String::new()),
    }
}

pub(super) fn read_optional_body(
    inline: Option<String>,
    body_file: Option<&Path>,
) -> Result<Option<String>, CliRunError> {
    match (inline, body_file) {
        (Some(body), None) => Ok(Some(body)),
        (None, Some(path)) =>
            fs::read_to_string(path).map(Some).map_err(|err| {
                CliRunError::BadRequest(format!(
                    "read body file {}: {err}",
                    path.display()
                ))
            }),
        (None, None) => Ok(None),
        (Some(_), Some(_)) => Err(CliRunError::BadRequest(
            "choose either --body or --body-file".to_string(),
        )),
    }
}

pub(super) fn default_section_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("imported")
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

pub(super) fn apply_source_location(
    manifest: &mut RuleManifest,
    source_repo: Option<&str>,
    source_path: Option<&str>,
    source_start_line: Option<i64>,
    source_end_line: Option<i64>,
) -> Result<(), CliRunError> {
    match (source_repo, source_path, source_start_line, source_end_line) {
        (Some(repo), Some(path), Some(start), Some(end)) => {
            manifest.set_source_location(repo, path, start, end);
            Ok(())
        }
        (None, None, None, None) => Ok(()),
        _ => Err(CliRunError::BadRequest(
            "source location requires --source-repo, --source-path, --source-start-line, and --source-end-line together".to_string(),
        )),
    }
}

pub(super) fn list_filter(args: &FilterArgs) -> RuleFilter {
    RuleFilter {
        state: args.state.clone(),
        file_kind: args.file_kind.clone(),
        section: args.section.clone(),
        repo_scope: args.repo_scope.clone(),
        path_scope: args.path_scope.clone(),
        slug: args.slug.clone(),
        has_unresolved_feedback: args.unresolved_only.then_some(true),
    }
}

pub(super) fn parse_fields(
    fields: &[String]
) -> Result<BTreeMap<String, Value>, CliRunError> {
    let mut patch = BTreeMap::new();
    for field in fields {
        let (key, value) = field.split_once('=').ok_or_else(|| {
            CliRunError::BadRequest(format!(
                "invalid field format '{field}', expected key=value"
            ))
        })?;
        patch.insert(
            key.trim().to_string(),
            Value::String(value.trim().to_string()),
        );
    }
    Ok(patch)
}

pub(super) fn rule_json(rule: &RuleManifest) -> Value {
    json!({
        "id": rule.id,
        "created_at": rule.created_at,
        "fields": &rule.extra,
    })
}

pub(super) fn rule_summary_json(rule: &RuleManifest) -> Value {
    json!({
        "id": rule.id,
        "slug": rule.slug(),
        "title": rule.title(),
        "state": rule.state(),
        "file_kind": rule.file_kind(),
        "section": rule.section(),
        "repo_scopes": rule.repo_scopes(),
        "path_scopes": rule.path_scopes(),
        "order_key": rule.order_key(),
        "feedback_unresolved_count": rule.feedback_unresolved_count(),
    })
}

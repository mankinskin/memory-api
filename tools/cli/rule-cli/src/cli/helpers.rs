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
    RuleCommandCli,
};

pub(super) fn resolve_index_root(
    explicit: Option<&Path>,
    workspace_root: Option<&Path>,
) -> PathBuf {
    let cwd = memory_api::workspace::working_dir();
    let env_root = std::env::var_os("RULE_INDEX_ROOT").map(PathBuf::from);
    resolve_index_root_from(
        explicit,
        workspace_root,
        env_root.as_deref(),
        cwd.as_deref(),
    )
}

fn resolve_index_root_from(
    explicit: Option<&Path>,
    workspace_root: Option<&Path>,
    env_root: Option<&Path>,
    cwd: Option<&Path>,
) -> PathBuf {
    memory_api::workspace::resolve_requested_store_root_from(
        explicit,
        workspace_root,
        env_root,
        cwd,
        ".rule",
    )
}

pub(super) fn resolve_workspace_root(
    command: &RuleCommandCli,
    index_root: &Path,
    workspace_root_override: Option<&Path>,
) -> Option<PathBuf> {
    workspace_root_override
        .map(|path| {
            let store_root =
                memory_api::workspace::resolve_store_root_from(path, ".rule");
            memory_api::workspace::resolve_workspace_root_from_store_root(
                &store_root,
                ".rule",
            )
        })
        .or_else(|| command_config_path(command))
        .or_else(|| rule_api::workspace_root_for_index_root(index_root))
        .or_else(|| {
            let cwd = memory_api::workspace::working_dir()?;
            cwd.join(".rule").exists().then_some(cwd)
        })
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
        has_low_feedback: args.low_rated_only.then_some(true),
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
        let value = value.trim();
        let parsed = if value.starts_with('[') || value.starts_with('{') {
            serde_json::from_str(value).map_err(|e| {
                CliRunError::BadRequest(format!(
                    "invalid JSON value for field '{key}': {e}"
                ))
            })?
        } else {
            Value::String(value.to_string())
        };
        patch.insert(key.trim().to_string(), parsed);
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

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn resolve_index_root_prefers_nearest_parent_rule_dir() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let nested = repo.join("tools").join("cli");
        std::fs::create_dir_all(repo.join(".rule")).unwrap();
        std::fs::create_dir_all(&nested).unwrap();

        let resolved = resolve_index_root_from(None, None, None, Some(&nested));

        assert_eq!(resolved, repo.join(".rule"));
    }

    #[test]
    fn resolve_index_root_defaults_to_current_directory_rule_dir() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("repo");
        std::fs::create_dir_all(&nested).unwrap();

        let resolved = resolve_index_root_from(None, None, None, Some(&nested));

        assert_eq!(resolved, nested.join(".rule"));
    }

    #[test]
    fn resolve_index_root_prefers_explicit_workspace_root() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let child = repo.join("memory-api");
        std::fs::create_dir_all(repo.join(".rule")).unwrap();
        std::fs::create_dir_all(child.join(".rule")).unwrap();

        let resolved =
            resolve_index_root_from(None, Some(&child), None, Some(&repo));

        assert_eq!(resolved, child.join(".rule"));
    }

    #[test]
    fn resolve_index_root_prefers_explicit_store_root_over_workspace_root() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let child = repo.join("memory-api");
        std::fs::create_dir_all(repo.join(".rule")).unwrap();
        std::fs::create_dir_all(child.join(".rule")).unwrap();

        let resolved = resolve_index_root_from(
            Some(&repo.join(".rule")),
            Some(&child),
            None,
            Some(&child),
        );

        assert_eq!(resolved, repo.join(".rule"));
    }
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
        "feedback_helpful_count": rule.feedback_helpful_count(),
        "feedback_mixed_count": rule.feedback_mixed_count(),
        "feedback_not_helpful_count": rule.feedback_not_helpful_count(),
        "feedback_note_count": rule.feedback_note_count(),
        "feedback_unresolved_count": rule.feedback_unresolved_count(),
        "feedback_last_at": rule.feedback_last_at(),
    })
}

fn command_config_path(command: &RuleCommandCli) -> Option<PathBuf> {
    let config = match command {
        RuleCommandCli::GenerateTarget(args) => Some(args.config.as_path()),
        RuleCommandCli::ExplainTarget(args) => Some(args.config.as_path()),
        RuleCommandCli::SyncTargets(args) => Some(args.config.as_path()),
        _ => None,
    }?;

    let cwd = memory_api::workspace::working_dir()?;
    let config = if config.is_absolute() {
        config.to_path_buf()
    } else {
        cwd.join(config)
    };

    config.parent().map(Path::to_path_buf)
}

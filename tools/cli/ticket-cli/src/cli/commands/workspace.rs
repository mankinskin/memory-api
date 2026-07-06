use std::path::{
    Path,
    PathBuf,
};

use serde_json::{
    Value,
    json,
};
use ticket_api::{
    storage::TicketStore,
    workspace_policy::{
        WorkspacePolicy,
        load_workspace_policy,
        load_workspace_policy_file,
        save_workspace_policy,
    },
};

use crate::cli::{
    CliRunError,
    WorkspaceArgs,
    WorkspaceCommand,
    WorkspacePatternArgs,
    WorkspacePatternCommand,
    WorkspacePolicyArgs,
    WorkspacePolicyCommand,
};

pub(crate) fn cmd_workspace(
    args: WorkspaceArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let workspace_root = workspace_root_of(store);
    match args.command {
        WorkspaceCommand::Policy(policy_args) =>
            cmd_workspace_policy(policy_args, &workspace_root),
        WorkspaceCommand::Ignore(pattern_args) => cmd_workspace_patterns(
            "ignore",
            pattern_args,
            &workspace_root,
        ),
        WorkspaceCommand::Include(pattern_args) => cmd_workspace_patterns(
            "include",
            pattern_args,
            &workspace_root,
        ),
        WorkspaceCommand::Rescan { apply_policy } =>
            cmd_workspace_rescan(apply_policy, store, &workspace_root),
    }
}

fn workspace_root_of(store: &TicketStore) -> PathBuf {
    ticket_api::workspace::resolve_workspace_root_from_store_root(
        &store.index_root,
        ticket_api::workspace::TICKET_INDEX_DIR,
    )
}

/// Load the on-disk policy for mutation, starting from the documented defaults
/// (not compatibility defaults) when no file exists yet. Malformed files fall
/// back to defaults so a `set`/`add` never silently drops all fields.
fn load_editable_policy(workspace_root: &Path) -> WorkspacePolicy {
    load_workspace_policy_file(workspace_root).unwrap_or_default()
}

fn policy_to_json(policy: &WorkspacePolicy) -> Value {
    json!({
        "include_descendants": policy.include_descendants,
        "include_ancestors": policy.include_ancestors,
        "deny_external_paths": policy.deny_external_paths,
        "ignore_workspaces": policy.ignore_workspaces,
        "include_overrides": policy.include_overrides,
        "ignore_markers": policy.ignore_markers,
    })
}

fn cmd_workspace_policy(
    args: WorkspacePolicyArgs,
    workspace_root: &Path,
) -> Result<Value, CliRunError> {
    match args.command {
        WorkspacePolicyCommand::Show => {
            let policy = load_workspace_policy(workspace_root);
            let source = if policy.compatibility_mode {
                "compatibility-defaults"
            } else {
                "file"
            };
            Ok(json!({
                "command": "workspace_policy_show",
                "status": "ok",
                "source": source,
                "workspace_root": workspace_root.display().to_string(),
                "policy": policy_to_json(&policy),
            }))
        },
        WorkspacePolicyCommand::Set {
            include_descendants,
            include_ancestors,
            deny_external_paths,
        } => {
            let mut policy = load_editable_policy(workspace_root);
            if let Some(value) = include_descendants {
                policy.include_descendants = value;
            }
            if let Some(value) = include_ancestors {
                policy.include_ancestors = value;
            }
            if let Some(value) = deny_external_paths {
                policy.deny_external_paths = value;
            }
            save_policy(workspace_root, &policy)?;
            Ok(json!({
                "command": "workspace_policy_set",
                "status": "ok",
                "workspace_root": workspace_root.display().to_string(),
                "policy": policy_to_json(&policy),
            }))
        },
    }
}

fn cmd_workspace_patterns(
    kind: &str,
    args: WorkspacePatternArgs,
    workspace_root: &Path,
) -> Result<Value, CliRunError> {
    let mut policy = load_editable_policy(workspace_root);
    let (command, changed) = match args.command {
        WorkspacePatternCommand::Add { pattern } => {
            let list = pattern_list_mut(kind, &mut policy);
            let changed = !list.iter().any(|entry| entry == &pattern);
            if changed {
                list.push(pattern);
            }
            (format!("workspace_{kind}_add"), changed)
        },
        WorkspacePatternCommand::Remove { pattern } => {
            let list = pattern_list_mut(kind, &mut policy);
            let before = list.len();
            list.retain(|entry| entry != &pattern);
            (format!("workspace_{kind}_remove"), list.len() != before)
        },
    };
    save_policy(workspace_root, &policy)?;
    Ok(json!({
        "command": command,
        "status": "ok",
        "changed": changed,
        "workspace_root": workspace_root.display().to_string(),
        "policy": policy_to_json(&policy),
    }))
}

fn pattern_list_mut<'a>(
    kind: &str,
    policy: &'a mut WorkspacePolicy,
) -> &'a mut Vec<String> {
    match kind {
        "ignore" => &mut policy.ignore_workspaces,
        _ => &mut policy.include_overrides,
    }
}

fn cmd_workspace_rescan(
    apply_policy: bool,
    store: &TicketStore,
    workspace_root: &Path,
) -> Result<Value, CliRunError> {
    if !apply_policy {
        return Err(CliRunError::BadRequest(
            "'workspace rescan' currently requires --apply-policy".to_string(),
        ));
    }
    let report = store.reapply_workspace_policy(workspace_root)?;
    Ok(json!({
        "command": "workspace_rescan",
        "status": "ok",
        "apply_policy": true,
        "workspace_root": workspace_root.display().to_string(),
        "integrated": report.integrated,
        "pruned": report.pruned,
        "skipped_roots": report.skipped_roots,
    }))
}

fn save_policy(
    workspace_root: &Path,
    policy: &WorkspacePolicy,
) -> Result<(), CliRunError> {
    save_workspace_policy(workspace_root, policy).map_err(|error| {
        CliRunError::BadRequest(format!(
            "failed to write workspace-policy.toml: {error}"
        ))
    })
}

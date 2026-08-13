use std::path::{
    Path,
    PathBuf,
};

use serde_json::{
    Value,
    json,
};
use spec_api::SpecStore;
use ticket_api::storage::TicketStore;
use uuid::Uuid;

use crate::cli::CliRunError;

use ticket_api::model::ticket::TicketManifestExt;

/// Resolve a `TicketRef.store_root` (repo-root-relative, e.g. ".ticket" or
/// "memory-api/.ticket") against the workspace root that `spec validate-links`
/// was invoked against.
fn resolve_referenced_root(
    workspace_root: &Path,
    store_root: &str,
) -> PathBuf {
    let candidate = Path::new(store_root);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        workspace_root.join(candidate)
    }
}

/// Attempt to read a ticket at `root` by id. Returns `None` when the store
/// does not exist there or the id is not found — both are "not resolved
/// here", distinguished from a hard error only by detection-rule context.
fn try_get_ticket(
    root: &Path,
    ticket_id: Uuid,
) -> Option<ticket_api::model::ticket::TicketManifest> {
    let store = TicketStore::open(root).ok()?;
    store.get(&ticket_id).ok()
}

pub(crate) fn cmd_validate_links(
    store: &SpecStore,
    workspace_root: &Path,
) -> Result<Value, CliRunError> {
    let canonical_ticket_root = workspace_root.join(".ticket");
    let all = store.entity_store().list_indexed()?;
    let mut findings: Vec<Value> = Vec::new();
    let mut checked = 0usize;

    for indexed in &all {
        let spec = match store.get(&indexed.id.to_string()) {
            Ok(spec) => spec,
            Err(_) => continue,
        };

        for ticket_ref in spec.related_tickets() {
            checked += 1;
            let referenced_root =
                resolve_referenced_root(workspace_root, &ticket_ref.store_root);

            if let Some(ticket) =
                try_get_ticket(&referenced_root, ticket_ref.ticket_id)
            {
                let has_back_ref = ticket
                    .related_specs()
                    .iter()
                    .any(|spec_ref| spec_ref.spec_id == spec.id());
                if !has_back_ref {
                    findings.push(json!({
                        "kind": "bidirectional_inconsistency",
                        "spec_id": spec.id(),
                        "ticket_id": ticket_ref.ticket_id,
                        "workspace": ticket_ref.workspace,
                        "store_root": ticket_ref.store_root,
                        "message": format!(
                            "spec {} links ticket {} but the ticket's related_specs does not link back",
                            spec.id(), ticket_ref.ticket_id,
                        ),
                    }));
                }
                continue;
            }

            // Not found at the referenced store_root. Check whether it
            // resolves under the workspace's canonical `.ticket` store
            // instead — this is the structural nested-store regression:
            // the id is real, just not where the reference claims.
            if referenced_root != canonical_ticket_root
                && try_get_ticket(&canonical_ticket_root, ticket_ref.ticket_id)
                    .is_some()
            {
                findings.push(json!({
                    "kind": "wrong_store_ref",
                    "spec_id": spec.id(),
                    "ticket_id": ticket_ref.ticket_id,
                    "workspace": ticket_ref.workspace,
                    "store_root": ticket_ref.store_root,
                    "message": format!(
                        "ticket {} exists but not under store_root '{}'; found under the workspace's canonical .ticket store instead",
                        ticket_ref.ticket_id, ticket_ref.store_root,
                    ),
                }));
                continue;
            }

            findings.push(json!({
                "kind": "dangling_ticket_ref",
                "spec_id": spec.id(),
                "ticket_id": ticket_ref.ticket_id,
                "workspace": ticket_ref.workspace,
                "store_root": ticket_ref.store_root,
                "message": format!(
                    "ticket {} not found under store_root '{}'",
                    ticket_ref.ticket_id, ticket_ref.store_root,
                ),
            }));
        }
    }

    let counts = count_by_kind(&findings);

    Ok(json!({
        "command": "validate_links",
        "status": "ok",
        "workspace_root": workspace_root.display().to_string(),
        "checked": checked,
        "valid": findings.is_empty(),
        "counts": counts,
        "findings": findings,
    }))
}

fn count_by_kind(findings: &[Value]) -> Value {
    let mut counts = serde_json::Map::new();
    for finding in findings {
        let kind = finding
            .get("kind")
            .and_then(|k| k.as_str())
            .unwrap_or("unknown")
            .to_string();
        let entry = counts.entry(kind).or_insert(json!(0));
        if let Some(n) = entry.as_u64() {
            *entry = json!(n + 1);
        }
    }
    Value::Object(counts)
}

#[cfg(test)]
mod tests {
    use ticket_api::model::ticket::SpecRef;
    use spec_api::{
        SpecManifest,
        TicketRef,
    };
    use tempfile::TempDir;
    use ticket_api::storage::TicketStore;

    use super::{
        SpecStore,
        cmd_validate_links,
    };

    /// Reproduces the nested-store bug from the spec side: a spec's
    /// `related_tickets` entry carries a `store_root` that does not resolve
    /// to any store, while the referenced ticket actually exists under the
    /// workspace's canonical `.ticket` store. `validate-links` must detect
    /// this as `wrong_store_ref` rather than silently treating it as
    /// dangling or resolving it against the wrong store.
    #[test]
    fn detects_wrong_store_ref_for_nested_store_bug_scenario() {
        let workspace = TempDir::new().unwrap();
        let workspace_root = workspace.path();

        let ticket_store =
            TicketStore::init(&workspace_root.join(".ticket")).unwrap();
        let mut spec_store =
            SpecStore::init(&workspace_root.join(".spec")).unwrap();

        let ticket_id = ticket_store
            .create(
                None,
                "task",
                Some("Nested store bug ticket"),
                None,
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let mut spec_manifest = SpecManifest::new(
            "traceability/nested-store-bug-spec-side",
            "Nested store bug spec (spec-side)",
            "spec-api",
        );
        // Wrong: the ticket actually lives under the canonical `.ticket`
        // store, but this ref claims a nonexistent nested store.
        spec_manifest.set_related_tickets(vec![TicketRef {
            ticket_id,
            workspace: "default".to_string(),
            store_root: "nested/.ticket".to_string(),
        }]);
        spec_store.create(&spec_manifest, "body", None).unwrap();

        let result =
            cmd_validate_links(&spec_store, workspace_root).unwrap();

        assert_eq!(result["valid"], false);
        assert_eq!(result["checked"], 1);
        let findings = result["findings"].as_array().unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0]["kind"], "wrong_store_ref");
        assert_eq!(findings[0]["ticket_id"], ticket_id.to_string());
    }

    #[test]
    fn detects_dangling_ticket_ref() {
        let workspace = TempDir::new().unwrap();
        let workspace_root = workspace.path();

        TicketStore::init(&workspace_root.join(".ticket")).unwrap();
        let mut spec_store =
            SpecStore::init(&workspace_root.join(".spec")).unwrap();

        let mut spec_manifest = SpecManifest::new(
            "traceability/dangling-ticket-ref",
            "Dangling ticket ref spec",
            "spec-api",
        );
        spec_manifest.set_related_tickets(vec![TicketRef {
            ticket_id: uuid::Uuid::new_v4(),
            workspace: "default".to_string(),
            store_root: ".ticket".to_string(),
        }]);
        spec_store.create(&spec_manifest, "body", None).unwrap();

        let result =
            cmd_validate_links(&spec_store, workspace_root).unwrap();

        assert_eq!(result["valid"], false);
        let findings = result["findings"].as_array().unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0]["kind"], "dangling_ticket_ref");
    }

    #[test]
    fn valid_when_bidirectional_link_is_consistent() {
        let workspace = TempDir::new().unwrap();
        let workspace_root = workspace.path();

        let ticket_store =
            TicketStore::init(&workspace_root.join(".ticket")).unwrap();
        let mut spec_store =
            SpecStore::init(&workspace_root.join(".spec")).unwrap();

        let mut spec_manifest = SpecManifest::new(
            "traceability/consistent-link-spec-side",
            "Consistent link spec (spec-side)",
            "spec-api",
        );
        let spec_id = spec_manifest.id();

        let ticket_id = ticket_store
            .create(
                None,
                "task",
                Some("Consistent link ticket"),
                None,
                Default::default(),
                None,
                None,
            )
            .unwrap();

        spec_manifest.set_related_tickets(vec![TicketRef {
            ticket_id,
            workspace: "default".to_string(),
            store_root: ".ticket".to_string(),
        }]);
        spec_store.create(&spec_manifest, "body", None).unwrap();

        let mut patch = std::collections::BTreeMap::new();
        patch.insert(
            "related_specs".to_string(),
            serde_json::to_value(vec![SpecRef {
                spec_id,
                workspace: "default".to_string(),
                store_root: ".spec".to_string(),
            }])
            .unwrap(),
        );
        ticket_store
            .update(&ticket_id, patch, None, None, None, None)
            .unwrap();

        let result =
            cmd_validate_links(&spec_store, workspace_root).unwrap();

        assert_eq!(result["valid"], true);
        assert_eq!(result["findings"].as_array().unwrap().len(), 0);
    }
}


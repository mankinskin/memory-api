use std::path::{
    Path,
    PathBuf,
};

use uuid::Uuid;

use crate::{
    discovery::discover_stores,
    model::index_entry::ContentKind,
    workspace::{
        discover_workspace_scan_roots_with_policy,
        resolve_store_root_from,
        resolve_workspace_root_from_store_root,
    },
    workspace_policy::WorkspacePolicy,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeReferenceResolution {
    Ok,
    CrossWorkspaceEdge {
        target_store_root: PathBuf,
        target_workspace_root: PathBuf,
    },
    DanglingEdge,
}

#[derive(Debug, Clone)]
pub struct CrossStoreEdgeClassifier {
    layout: StoreLayout,
    included_store_roots: Vec<PathBuf>,
    discoverable_store_roots: Vec<PathBuf>,
}

impl CrossStoreEdgeClassifier {
    pub fn for_store(
        active_store_root: &Path,
        kind: ContentKind,
        policy: WorkspacePolicy,
    ) -> Option<Self> {
        let layout = StoreLayout::for_kind(kind)?;
        let active_store_root =
            resolve_store_root_from(active_store_root, layout.store_dir);
        let active_workspace_root = resolve_workspace_root_from_store_root(
            &active_store_root,
            layout.store_dir,
        );

        let mut included_store_roots =
            discover_workspace_scan_roots_with_policy(
                &active_workspace_root,
                layout.store_dir,
                layout.entity_dir,
                &policy,
            )
            .into_iter()
            .map(|root| resolve_store_root_from(&root.path, layout.store_dir))
            .collect::<Vec<_>>();
        if !included_store_roots.contains(&active_store_root) {
            included_store_roots.push(active_store_root.clone());
        }
        included_store_roots.sort();
        included_store_roots.dedup();

        let mut discoverable_store_roots = included_store_roots.clone();
        discoverable_store_roots.extend(
            discover_stores(&active_workspace_root)
                .into_iter()
                .filter(|store| store.kind == kind)
                .map(|store| {
                    resolve_store_root_from(&store.store_root, layout.store_dir)
                }),
        );

        for ancestor in active_workspace_root.ancestors().skip(1) {
            let candidate = ancestor.join(layout.store_dir);
            if candidate.is_dir() {
                discoverable_store_roots.push(resolve_store_root_from(
                    &candidate,
                    layout.store_dir,
                ));
            }
        }

        discoverable_store_roots.sort();
        discoverable_store_roots.dedup();

        Some(Self {
            layout,
            included_store_roots,
            discoverable_store_roots,
        })
    }

    pub fn classify(
        &self,
        target_id: Uuid,
    ) -> EdgeReferenceResolution {
        if self
            .included_store_roots
            .iter()
            .any(|root| entity_exists(root, self.layout, target_id))
        {
            return EdgeReferenceResolution::Ok;
        }

        if let Some(root) = self
            .discoverable_store_roots
            .iter()
            .find(|root| entity_exists(root, self.layout, target_id))
        {
            return EdgeReferenceResolution::CrossWorkspaceEdge {
                target_store_root: root.clone(),
                target_workspace_root: resolve_workspace_root_from_store_root(
                    root,
                    self.layout.store_dir,
                ),
            };
        }

        EdgeReferenceResolution::DanglingEdge
    }
}

pub fn short_id8(id: Uuid) -> String {
    id.to_string()[..8].to_string()
}

pub fn cross_workspace_edge_message(
    target_id: Uuid,
    target_workspace_root: &Path,
) -> String {
    format!(
        "depends_on edge points to {} in workspace '{}' which is outside the active workspace policy scope.",
        short_id8(target_id),
        target_workspace_root.display()
    )
}

pub fn cross_workspace_edge_instructions() -> Vec<String> {
    vec![
        "Remove or retarget the depends_on edge to keep dependencies within the active workspace policy scope.".to_string(),
        "Move the entity so the dependency becomes intra-policy once move tooling is available.".to_string(),
        "If this cross-workspace dependency is intentional, enable ancestor indexing in workspace-policy.toml (include_ancestors = true).".to_string(),
    ]
}

#[derive(Debug, Copy, Clone)]
struct StoreLayout {
    store_dir: &'static str,
    entity_dir: &'static str,
    manifest_file: &'static str,
}

impl StoreLayout {
    fn for_kind(kind: ContentKind) -> Option<Self> {
        match kind {
            ContentKind::Ticket => Some(Self {
                store_dir: ".ticket",
                entity_dir: "tickets",
                manifest_file: "ticket.toml",
            }),
            ContentKind::Spec => Some(Self {
                store_dir: ".spec",
                entity_dir: "specs",
                manifest_file: "spec.toml",
            }),
            ContentKind::Rule => Some(Self {
                store_dir: ".rule",
                entity_dir: "rules",
                manifest_file: "rule.toml",
            }),
            _ => None,
        }
    }
}

fn entity_exists(
    store_root: &Path,
    layout: StoreLayout,
    id: Uuid,
) -> bool {
    store_root
        .join(layout.entity_dir)
        .join(id.to_string())
        .join(layout.manifest_file)
        .is_file()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::workspace_policy::WorkspacePolicy;

    use super::*;

    fn write_entity(
        workspace_root: &Path,
        kind: ContentKind,
        id: Uuid,
    ) {
        let layout = StoreLayout::for_kind(kind).unwrap();
        let path = workspace_root
            .join(layout.store_dir)
            .join(layout.entity_dir)
            .join(id.to_string());
        fs::create_dir_all(&path).unwrap();
        fs::write(
            path.join(layout.manifest_file),
            "id = \"placeholder\"\n",
        )
        .unwrap();
    }

    #[test]
    fn descendant_store_resolution_is_ok() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("repo");
        let child = root.join("child");
        fs::create_dir_all(&child).unwrap();

        let target_id = Uuid::new_v4();
        write_entity(&child, ContentKind::Ticket, target_id);

        let classifier = CrossStoreEdgeClassifier::for_store(
            &root.join(".ticket"),
            ContentKind::Ticket,
            WorkspacePolicy::compatibility_default(),
        )
        .unwrap();

        assert_eq!(
            classifier.classify(target_id),
            EdgeReferenceResolution::Ok
        );
    }

    #[test]
    fn ancestor_store_resolution_becomes_cross_workspace_warning() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("repo");
        let child = root.join("child");
        fs::create_dir_all(&child).unwrap();

        let target_id = Uuid::new_v4();
        write_entity(&root, ContentKind::Ticket, target_id);

        let policy = WorkspacePolicy {
            include_descendants: true,
            include_ancestors: true,
            deny_external_paths: true,
            ..WorkspacePolicy::default()
        };

        let classifier = CrossStoreEdgeClassifier::for_store(
            &child.join(".ticket"),
            ContentKind::Ticket,
            policy,
        )
        .unwrap();

        match classifier.classify(target_id) {
            EdgeReferenceResolution::CrossWorkspaceEdge {
                target_workspace_root,
                ..
            } => assert_eq!(target_workspace_root, root),
            other => panic!("expected CrossWorkspaceEdge, got {other:?}"),
        }
    }

    #[test]
    fn missing_target_is_dangling() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("repo");
        fs::create_dir_all(&root).unwrap();

        let classifier = CrossStoreEdgeClassifier::for_store(
            &root.join(".ticket"),
            ContentKind::Ticket,
            WorkspacePolicy::compatibility_default(),
        )
        .unwrap();

        assert_eq!(
            classifier.classify(Uuid::new_v4()),
            EdgeReferenceResolution::DanglingEdge
        );
    }

    #[test]
    fn ticket_and_spec_classification_parity() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("repo");
        let child = root.join("child");
        fs::create_dir_all(&child).unwrap();

        let ticket_id = Uuid::new_v4();
        let spec_id = Uuid::new_v4();
        write_entity(&root, ContentKind::Ticket, ticket_id);
        write_entity(&root, ContentKind::Spec, spec_id);

        let policy = WorkspacePolicy {
            include_descendants: true,
            include_ancestors: true,
            deny_external_paths: true,
            ..WorkspacePolicy::default()
        };

        let ticket = CrossStoreEdgeClassifier::for_store(
            &child.join(".ticket"),
            ContentKind::Ticket,
            policy.clone(),
        )
        .unwrap();
        let spec = CrossStoreEdgeClassifier::for_store(
            &child.join(".spec"),
            ContentKind::Spec,
            policy,
        )
        .unwrap();

        assert!(matches!(
            ticket.classify(ticket_id),
            EdgeReferenceResolution::CrossWorkspaceEdge { .. }
        ));
        assert!(matches!(
            spec.classify(spec_id),
            EdgeReferenceResolution::CrossWorkspaceEdge { .. }
        ));
    }
}

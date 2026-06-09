//! Memory workspace DAG indexing (ticket `c2409055`).
//!
//! Each tool domain workspace (`.ticket/`, `.spec/`, `.rule/`, etc.) is a DAG
//! node with multiple parents and multiple children (D9). This module generates
//! a [`ContentKind::WorkspaceSummary`] [`IndexEntry`] for a single workspace
//! node, including parent and child workspace refs.
//!
//! There is **no** global `.context/` store. Each workspace emits its own
//! isolated summary entry; the DAG topology is reconstructed by reading each
//! workspace's sidecar and following `parent`/`child` refs.

use std::path::Path;

use chrono::Utc;
use uuid::Uuid;

use crate::model::index_entry::{
    ContentKind,
    IndexEntry,
    IndexRef,
    IndexRelations,
    RelationKind,
};
use crate::model::index_sidecar::IndexSidecar;

use super::ticket::deterministic_uuid;

/// Namespace UUID for deterministic workspace node UUIDs.
const WORKSPACE_NS: Uuid = Uuid::from_bytes([
    0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
    0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00,
]);

/// A reference to a neighbouring workspace node (parent or child).
pub struct WorkspaceRef {
    /// Human-readable name of the workspace (e.g. `"ticket"`, `"spec"`).
    pub name: String,
    /// Workspace-relative path to the neighbour's store folder (e.g. `"../.spec"`).
    pub relative_path: String,
    /// Content domain of the neighbour.
    pub domain: ContentKind,
}

/// Input for the workspace DAG sidecar generator.
pub struct WorkspaceIndexInput<'a> {
    /// Human-readable name of this workspace (e.g. `"ticket"`).
    pub name: &'a str,
    /// Store folder relative to repo root (e.g. `.ticket`).
    pub store_dir: &'a str,
    /// Domain kind of this workspace.
    pub domain: ContentKind,
    /// Freshness/health summary string (counts, last-generated timestamp, etc.).
    pub summary: &'a str,
    /// Parent workspaces in the DAG (may be empty).
    pub parents: &'a [WorkspaceRef],
    /// Child workspaces in the DAG (may be empty).
    pub children: &'a [WorkspaceRef],
    /// Workspace root for relative path computation.
    pub workspace_root: &'a Path,
}

/// Generate an [`IndexSidecar`] for a single workspace DAG node.
///
/// Emits exactly one [`IndexEntry`] of kind [`ContentKind::WorkspaceSummary`]
/// with parent and child workspace refs populated from `input.parents` /
/// `input.children`.
///
/// The entry id is deterministic: given the same `store_dir` slug the UUID is
/// identical across runs and platforms.
pub fn generate_workspace_sidecar(input: WorkspaceIndexInput<'_>) -> IndexSidecar {
    let node_id = deterministic_uuid(WORKSPACE_NS, input.store_dir);

    let parent_refs: Vec<IndexRef> = input.parents.iter().map(|p| {
        IndexRef {
            canonical_path: p.relative_path.clone(),
            entry_id: deterministic_uuid(WORKSPACE_NS, &p.relative_path),
            relation_kind: RelationKind::Parent,
            content_kind: p.domain,
            digest: String::new(),
            anchor: None,
        }
    }).collect();

    let child_refs: Vec<IndexRef> = input.children.iter().map(|c| {
        IndexRef {
            canonical_path: c.relative_path.clone(),
            entry_id: deterministic_uuid(WORKSPACE_NS, &c.relative_path),
            relation_kind: RelationKind::Child,
            content_kind: c.domain,
            digest: String::new(),
            anchor: None,
        }
    }).collect();

    let mut tags = vec![input.name.to_string(), "workspace".to_string()];
    tags.sort_unstable();
    tags.dedup();

    let mut entry = IndexEntry {
        id: node_id,
        kind: ContentKind::WorkspaceSummary,
        source_path: format!("{}/index.toon", input.store_dir),
        title: format!("{} workspace", input.name),
        summary: input.summary.to_string(),
        keywords: vec![input.name.to_string(), "workspace".to_string()],
        scope: Some(input.store_dir.to_string()),
        non_goals: None,
        relations: IndexRelations {
            parent: parent_refs.into_iter().next(),
            children: child_refs,
            depends_on: vec![],
            related: vec![],
        },
        digest: String::new(),
        tags,
        generated_at: Utc::now(),
        source_modified_at: None,
    };
    entry.seal();

    let mut sidecar = IndexSidecar::new(
        input.domain,
        input.store_dir,
        vec![entry],
    );
    sidecar.sort();
    sidecar
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn workspace_entry_id_is_deterministic() {
        let ws = PathBuf::from("/workspace");
        let s1 = generate_workspace_sidecar(WorkspaceIndexInput {
            name: "ticket",
            store_dir: ".ticket",
            domain: ContentKind::Ticket,
            summary: "5 tickets",
            parents: &[],
            children: &[],
            workspace_root: &ws,
        });
        let s2 = generate_workspace_sidecar(WorkspaceIndexInput {
            name: "ticket",
            store_dir: ".ticket",
            domain: ContentKind::Ticket,
            summary: "6 tickets", // changed — should not affect id
            parents: &[],
            children: &[],
            workspace_root: &ws,
        });
        assert_eq!(s1.entries[0].id, s2.entries[0].id);
    }

    #[test]
    fn workspace_entry_with_parents_and_children() {
        let ws = PathBuf::from("/workspace");
        let parents = vec![WorkspaceRef {
            name: "root".to_string(),
            relative_path: "..".to_string(),
            domain: ContentKind::WorkspaceSummary,
        }];
        let children = vec![WorkspaceRef {
            name: "spec".to_string(),
            relative_path: "../.spec".to_string(),
            domain: ContentKind::Spec,
        }];
        let sidecar = generate_workspace_sidecar(WorkspaceIndexInput {
            name: "ticket",
            store_dir: ".ticket",
            domain: ContentKind::Ticket,
            summary: "5 tickets",
            parents: &parents,
            children: &children,
            workspace_root: &ws,
        });
        let entry = &sidecar.entries[0];
        assert!(entry.relations.parent.is_some());
        assert_eq!(entry.relations.children.len(), 1);
        assert!(entry.is_digest_valid());
    }

    #[test]
    fn different_store_dirs_produce_different_ids() {
        let ws = PathBuf::from("/workspace");
        let make = |store_dir: &str| {
            generate_workspace_sidecar(WorkspaceIndexInput {
                name: store_dir,
                store_dir,
                domain: ContentKind::Ticket,
                summary: "",
                parents: &[],
                children: &[],
                workspace_root: &ws,
            }).entries[0].id
        };
        assert_ne!(make(".ticket"), make(".spec"));
    }
}

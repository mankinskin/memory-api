use super::*;

    use super::*;
    use crate::manifest::SpecManifest;

    fn spec(
        slug: &str,
        title: &str,
        component: &str,
    ) -> SpecManifest {
        let mut m = SpecManifest::new(slug, title, component);
        m.set_scope("internal");
        m
    }

    fn source<'a>(
        manifest: &'a SpecManifest,
        path: &str,
        body: &str,
    ) -> SpecCatalogSource<'a> {
        SpecCatalogSource {
            manifest,
            source_path: path.to_string(),
            body: body.to_string(),
        }
    }

    #[test]
    fn summary_takes_first_text_block() {
        assert_eq!(
            normalize_summary("# Heading\n\nThe contract goal.\n"),
            "The contract goal."
        );
        assert_eq!(normalize_summary("## Only a heading"), "");
    }

    #[test]
    fn extract_section_reads_named_section() {
        let body = "# Goal\n\nDo X.\n\n## Scope\n\nThe API surface.\n\n## Non-goals\n\nNot the UI.\n";
        assert_eq!(
            extract_section(body, "Scope").as_deref(),
            Some("The API surface.")
        );
        assert_eq!(
            extract_section(body, "Non-goals").as_deref(),
            Some("Not the UI.")
        );
        assert_eq!(extract_section(body, "Missing"), None);
    }

    #[test]
    fn hierarchy_relations_are_populated() {
        let parent = spec("root", "Root", "comp-a");
        let parent_id = parent.id.to_string();
        let mut child = spec("root/child", "Child", "comp-a");
        child.set_parent(&parent_id);

        let sources = vec![
            source(&parent, ".spec/specs/root/spec.toml", "Root body."),
            source(&child, ".spec/specs/child/spec.toml", "Child body."),
        ];

        let artifacts = generate_spec_catalog(&sources, ".spec");
        let by_id: std::collections::HashMap<_, _> = artifacts
            .sidecar
            .entries
            .iter()
            .map(|e| (e.id, e))
            .collect();

        let parent_entry = by_id[&parent.id];
        let child_entry = by_id[&child.id];

        // Parent has one child ref; child has a parent ref.
        assert_eq!(parent_entry.relations.children.len(), 1);
        assert_eq!(parent_entry.relations.children[0].entry_id, child.id);
        assert_eq!(
            parent_entry.relations.children[0].relation_kind,
            RelationKind::Child
        );
        assert!(parent_entry.tags.iter().any(|t| t == "root"));

        let parent_ref = child_entry.relations.parent.as_ref().unwrap();
        assert_eq!(parent_ref.entry_id, parent.id);
        assert_eq!(parent_ref.relation_kind, RelationKind::Parent);
        assert!(!child_entry.tags.iter().any(|t| t == "root"));
    }

    #[test]
    fn catalog_has_provenance_grouping_and_hierarchy_bullets() {
        let parent = spec("root", "Root", "comp-a");
        let parent_id = parent.id.to_string();
        let mut child = spec("root/child", "Child", "comp-a");
        child.set_parent(&parent_id);

        let sources = vec![
            source(&parent, ".spec/specs/root/spec.toml", "Root body."),
            source(&child, ".spec/specs/child/spec.toml", "Child body."),
        ];

        let artifacts = generate_spec_catalog(&sources, ".spec");
        let md = &artifacts.readme_markdown;
        assert!(md.starts_with(SPEC_INDEX_FILE_COMMENT));
        assert!(md.contains("## comp-a"));
        assert!(md.contains("- [root](./tree/root/"));
        assert!(md.contains("- [root/child](./tree/root/"));

        assert_eq!(artifacts.tree_markdown.len(), 2);
        let parent_tree = artifacts
            .tree_markdown
            .iter()
            .find(|(k, _)| k.contains("/root/"))
            .map(|(_, v)| v)
            .unwrap();
        assert!(parent_tree.contains(SPEC_INDEX_TREE_ENTRY_COMMENT));
        assert!(parent_tree.contains("## Navigation"));
        assert!(parent_tree.contains("Children:"));
        for e in &artifacts.sidecar.entries {
            assert!(e.is_digest_valid());
        }
    }

    #[test]
    fn regeneration_is_byte_stable() {
        let parent = spec("root", "Root", "comp-a");
        let sources =
            vec![source(&parent, ".spec/specs/root/spec.toml", "Body.")];

        let a = generate_spec_catalog(&sources, ".spec");
        let b = generate_spec_catalog(&sources, ".spec");
        assert_eq!(a.readme_markdown, b.readme_markdown);
        assert_eq!(
            a.sidecar.encode_toon().unwrap(),
            b.sidecar.encode_toon().unwrap()
        );
        assert_eq!(a.agent_hook_markdown, b.agent_hook_markdown);
    }

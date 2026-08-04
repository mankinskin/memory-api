use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
};

const INSTALL_CONTRACT_SLUG: &str =
    "memory-api/install-contracts/cli-and-viewer-installation";
// The rule store was retired in favor of hand-maintained instructions
// (commit 99820bf6); the "## Tool Use Examples" section of the top-level
// README.md is now the canonical hand-maintained copy of this content.
const README_SECTION_HEADING: &str = "## Tool Use Examples";

fn memory_api_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("memory-api root should exist")
}

fn install_contract_dir() -> PathBuf {
    let specs_root = memory_api_root().join(".spec").join("specs");
    let entries =
        fs::read_dir(&specs_root).expect("specs directory should be readable");

    for entry in entries.flatten() {
        let path = entry.path();
        let spec_toml = path.join("spec.toml");
        let Ok(contents) = fs::read_to_string(&spec_toml) else {
            continue;
        };
        if contents.contains(&format!("slug = \"{INSTALL_CONTRACT_SLUG}\"")) {
            return path;
        }
    }

    panic!(
        "could not find install contract spec for slug {INSTALL_CONTRACT_SLUG}"
    );
}

fn section(name: &str) -> String {
    fs::read_to_string(
        install_contract_dir()
            .join("sections")
            .join(format!("{name}.md")),
    )
    .unwrap_or_else(|_| panic!("missing section {name}"))
}

fn normalize_newlines(input: &str) -> String {
    input.replace("\r\n", "\n")
}

/// Extracts a top-level (`## `) section from README.md by heading text,
/// returning the heading line through the next `## ` heading or EOF.
fn readme_section(heading: &str) -> String {
    let readme = fs::read_to_string(memory_api_root().join("README.md"))
        .expect("memory-api README.md should exist");
    let normalized = normalize_newlines(&readme);
    let start = normalized
        .find(heading)
        .unwrap_or_else(|| panic!("README.md missing heading {heading}"));
    let after_heading = &normalized[start + heading.len()..];
    let end = after_heading
        .find("\n## ")
        .map(|offset| start + heading.len() + offset)
        .unwrap_or(normalized.len());
    normalized[start..end].trim_end().to_string()
}

#[test]
fn readme_install_flow_section_matches_readme_rule_entry() {
    let expected = section("readme-install-flow");
    let actual = readme_section(README_SECTION_HEADING);

    assert_eq!(actual, normalize_newlines(&expected).trim_end());
}

#[test]
fn install_contract_sections_record_cli_and_viewer_matrix() {
    let cli = section("cli-scenario-matrix");
    for scenario in ["CLI-01", "CLI-02", "CLI-03", "CLI-04", "CLI-05"] {
        assert!(cli.contains(scenario), "missing CLI scenario {scenario}");
    }
    assert!(cli.contains("cargo install --path tools/cli/rule-cli --bin rule"));
    assert!(cli.contains("cargo uninstall rule-cli"));
    assert!(cli.contains("rule list"));
    assert!(cli.contains("rule create --title \"Install validation rule\""));
    assert!(cli.contains("spec create --title \"Install validation spec\""));
    assert!(cli.contains("ticket create --title \"Install validation ticket\" --type tracker-improvement"));
    assert!(cli.contains("audit run ."));

    let viewer = section("viewer-install-boundary");
    for scenario in ["VIEW-01", "VIEW-02", "VIEW-03", "VIEW-04"] {
        assert!(
            viewer.contains(scenario),
            "missing viewer scenario {scenario}"
        );
    }
    assert!(viewer.contains("viewer-ctl install doc-viewer --kind server"));
    assert!(viewer.contains("viewer-ctl install doc-viewer --kind frontend"));
    assert!(viewer.contains("viewer-ctl install log-viewer --kind server"));
    assert!(viewer.contains("viewer-ctl install log-viewer --kind frontend"));
    assert!(viewer.contains("viewer-ctl install ticket-viewer --kind server"));
    assert!(
        viewer.contains("viewer-ctl install ticket-viewer --kind frontend")
    );
    assert!(viewer.contains("viewer-ctl install spec-viewer --kind server"));
    assert!(viewer.contains("viewer-ctl install spec-viewer --kind frontend"));
    assert!(viewer.contains("No first-class uninstall command exists"));
}

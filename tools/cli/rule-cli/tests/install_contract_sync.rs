use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
};

const INSTALL_CONTRACT_SLUG: &str =
    "memory-api/install-contracts/cli-and-viewer-installation";
const README_RULE_PATH: &str =
    ".rule/rules/84278ede-0aaa-4382-83db-e6ee5d80106c/body.md";

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

#[test]
fn readme_install_flow_section_matches_readme_rule_entry() {
    let expected = section("readme-install-flow");
    let actual =
        fs::read_to_string(memory_api_root().join(README_RULE_PATH)).unwrap();

    assert_eq!(normalize_newlines(&actual), normalize_newlines(&expected));
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

use std::path::{
    Path,
    PathBuf,
};

use serde::{
    Deserialize,
    Serialize,
};

use crate::workspace::{
    DocPackage,
    DocTarget,
    DocWorkspace,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoDocArtifact {
    pub package_name: String,
    pub package_root: PathBuf,
    pub target_name: String,
    pub target_kind: Vec<String>,
    pub html_root_dir: PathBuf,
    pub html_index_path: PathBuf,
    pub html_exists: bool,
    pub rustdoc_json_path: PathBuf,
    pub rustdoc_json_exists: bool,
}

impl CargoDocArtifact {
    pub fn any_output_exists(&self) -> bool {
        self.html_exists || self.rustdoc_json_exists
    }
}

impl DocWorkspace {
    pub fn cargo_doc_root(&self) -> PathBuf {
        self.target_directory.join("doc")
    }

    pub fn cargo_doc_artifacts(&self) -> Vec<CargoDocArtifact> {
        let doc_root = self.cargo_doc_root();
        let mut artifacts = self
            .packages
            .iter()
            .flat_map(|package| package.cargo_doc_artifacts(&doc_root))
            .collect::<Vec<_>>();

        artifacts.sort_by(|left, right| {
            left.package_name
                .cmp(&right.package_name)
                .then_with(|| left.target_name.cmp(&right.target_name))
                .then_with(|| left.html_index_path.cmp(&right.html_index_path))
        });

        artifacts
    }
}

impl DocPackage {
    pub fn cargo_doc_artifacts(
        &self,
        doc_root: &Path,
    ) -> Vec<CargoDocArtifact> {
        self.targets
            .iter()
            .filter(|target| target.doc_capable)
            .map(|target| target.cargo_doc_artifact(&self.name, &self.package_root, doc_root))
            .collect()
    }
}

impl DocTarget {
    pub fn cargo_doc_artifact(
        &self,
        package_name: &str,
        package_root: &Path,
        doc_root: &Path,
    ) -> CargoDocArtifact {
        let stem = self.rustdoc_output_stem();
        let html_root_dir = doc_root.join(&stem);
        let html_index_path = html_root_dir.join("index.html");
        let rustdoc_json_path = doc_root.join(format!("{stem}.json"));

        CargoDocArtifact {
            package_name: package_name.to_string(),
            package_root: package_root.to_path_buf(),
            target_name: self.name.clone(),
            target_kind: self.kind.clone(),
            html_exists: html_index_path.exists(),
            html_root_dir,
            html_index_path,
            rustdoc_json_exists: rustdoc_json_path.exists(),
            rustdoc_json_path,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::Path,
    };

    use cargo_metadata::MetadataCommand;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    use crate::workspace::DocWorkspace;

    #[test]
    fn discovers_existing_cargo_doc_html_and_json_outputs() {
        let dir = temp_workspace();
        write_file(
            &dir
                .path()
                .join("target/doc/alpha_crate/index.html"),
            "<html>alpha</html>",
        );
        write_file(
            &dir.path().join("target/doc/alpha_crate.json"),
            "{}",
        );
        write_file(
            &dir.path().join("target/doc/beta_tool/index.html"),
            "<html>beta</html>",
        );

        let workspace = DocWorkspace::from_cargo_metadata(cargo_metadata_for(dir.path())).unwrap();
        let artifacts = workspace.cargo_doc_artifacts();

        assert_eq!(artifacts.len(), 2);

        let alpha = artifacts.iter().find(|artifact| artifact.package_name == "alpha-crate").unwrap();
        assert_eq!(alpha.target_name, "alpha_crate");
        assert!(alpha.html_exists);
        assert!(alpha.rustdoc_json_exists);
        assert!(alpha.any_output_exists());
        assert_eq!(alpha.html_index_path, dir.path().join("target/doc/alpha_crate/index.html"));
        assert_eq!(alpha.rustdoc_json_path, dir.path().join("target/doc/alpha_crate.json"));

        let beta = artifacts.iter().find(|artifact| artifact.package_name == "beta-tool").unwrap();
        assert_eq!(beta.target_name, "beta-tool");
        assert!(beta.html_exists);
        assert!(!beta.rustdoc_json_exists);
        assert!(beta.any_output_exists());
        assert_eq!(beta.html_index_path, dir.path().join("target/doc/beta_tool/index.html"));
        assert_eq!(beta.rustdoc_json_path, dir.path().join("target/doc/beta_tool.json"));
    }

    #[test]
    fn reports_expected_paths_when_outputs_are_missing() {
        let dir = temp_workspace();
        let workspace = DocWorkspace::from_cargo_metadata(cargo_metadata_for(dir.path())).unwrap();

        let artifacts = workspace.cargo_doc_artifacts();

        assert_eq!(artifacts.len(), 2);
        assert!(artifacts.iter().all(|artifact| !artifact.html_exists));
        assert!(artifacts.iter().all(|artifact| !artifact.rustdoc_json_exists));
        assert!(artifacts.iter().all(|artifact| !artifact.any_output_exists()));
        assert_eq!(workspace.cargo_doc_root(), dir.path().join("target/doc"));
    }

    fn temp_workspace() -> tempfile::TempDir {
        let dir = tempdir().unwrap();

        write_file(
            &dir.path().join("Cargo.toml"),
            r#"[workspace]
members = ["alpha", "beta"]
resolver = "2"
"#,
        );

        write_file(
            &dir.path().join("alpha/Cargo.toml"),
            r#"[package]
name = "alpha-crate"
version = "0.1.0"
edition = "2024"
"#,
        );
        write_file(
            &dir.path().join("alpha/src/lib.rs"),
            "pub fn alpha() -> &'static str { \"alpha\" }\n",
        );

        write_file(
            &dir.path().join("beta/Cargo.toml"),
            r#"[package]
name = "beta-tool"
version = "0.2.0"
edition = "2024"
"#,
        );
        write_file(
            &dir.path().join("beta/src/main.rs"),
            "fn main() {}\n",
        );

        dir
    }

    fn cargo_metadata_for(root: &Path) -> cargo_metadata::Metadata {
        MetadataCommand::new()
            .current_dir(root)
            .no_deps()
            .exec()
            .unwrap()
    }

    fn write_file(
        path: &Path,
        contents: &str,
    ) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }
}
use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use cargo_metadata::{
    Metadata,
    MetadataCommand,
    Package,
    Target,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::error::DocError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocWorkspaceSource {
    CargoMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocWorkspace {
    pub source: DocWorkspaceSource,
    pub workspace_root: PathBuf,
    pub workspace_manifest_path: PathBuf,
    pub target_directory: PathBuf,
    pub packages: Vec<DocPackage>,
}

impl DocWorkspace {
    pub fn discover_from(
        root: impl AsRef<Path>,
    ) -> Result<Self, DocError> {
        let metadata = MetadataCommand::new()
            .current_dir(root.as_ref())
            .no_deps()
            .exec()
            .map_err(|err| DocError::CargoMetadata(err.to_string()))?;
        Self::from_cargo_metadata(metadata)
    }

    pub fn from_cargo_metadata(
        metadata: Metadata,
    ) -> Result<Self, DocError> {
        metadata.try_into()
    }

    pub fn from_cargo_metadata_json(
        json: &str,
    ) -> Result<Self, DocError> {
        let metadata: Metadata = serde_json::from_str(json)
            .map_err(|err| DocError::CargoMetadata(err.to_string()))?;
        Self::from_cargo_metadata(metadata)
    }

    pub fn from_cargo_metadata_file(
        path: impl AsRef<Path>,
    ) -> Result<Self, DocError> {
        let path = path.as_ref();
        let json = fs::read_to_string(path).map_err(|source| DocError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_cargo_metadata_json(&json)
    }

    pub fn package(
        &self,
        name: &str,
    ) -> Option<&DocPackage> {
        self.packages.iter().find(|package| package.name == name)
    }
}

impl TryFrom<Metadata> for DocWorkspace {
    type Error = DocError;

    fn try_from(metadata: Metadata) -> Result<Self, Self::Error> {
        let workspace_root = metadata.workspace_root.clone().into_std_path_buf();
        let target_directory =
            metadata.target_directory.clone().into_std_path_buf();
        let workspace_manifest_path = workspace_root.join("Cargo.toml");

        let mut packages = metadata
            .workspace_packages()
            .iter()
            .map(|package| DocPackage::try_from(*package))
            .collect::<Result<Vec<_>, _>>()?;

        if packages.is_empty() {
            return Err(DocError::EmptyWorkspace);
        }

        packages.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.manifest_path.cmp(&right.manifest_path))
        });

        Ok(Self {
            source: DocWorkspaceSource::CargoMetadata,
            workspace_root,
            workspace_manifest_path,
            target_directory,
            packages,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocPackage {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub manifest_path: PathBuf,
    pub package_root: PathBuf,
    pub targets: Vec<DocTarget>,
}

impl DocPackage {
    pub fn default_doc_target(&self) -> Option<&DocTarget> {
        self.targets
            .iter()
            .find(|target| target.doc_capable && target.is_library_like())
            .or_else(|| self.targets.iter().find(|target| target.doc_capable))
    }
}

impl TryFrom<&Package> for DocPackage {
    type Error = DocError;

    fn try_from(package: &Package) -> Result<Self, Self::Error> {
        let manifest_path = package.manifest_path.clone().into_std_path_buf();
        let package_root = manifest_path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| DocError::InvalidManifestPath(manifest_path.clone()))?;

        let mut targets = package
            .targets
            .iter()
            .map(DocTarget::from)
            .collect::<Vec<_>>();
        targets.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.src_path.cmp(&right.src_path))
        });

        Ok(Self {
            id: package.id.repr.clone(),
            name: package.name.clone(),
            version: package.version.to_string(),
            description: package.description.clone(),
            manifest_path,
            package_root,
            targets,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocTarget {
    pub name: String,
    pub kind: Vec<String>,
    pub crate_types: Vec<String>,
    pub src_path: PathBuf,
    pub edition: String,
    pub doc_capable: bool,
    pub doctest: bool,
    pub test: bool,
}

impl DocTarget {
    pub fn is_library_like(&self) -> bool {
        self.kind
            .iter()
            .any(|kind| matches!(kind.as_str(), "lib" | "proc-macro"))
    }

    pub fn rustdoc_output_stem(&self) -> String {
        self.name.replace('-', "_")
    }
}

impl From<&Target> for DocTarget {
    fn from(target: &Target) -> Self {
        Self {
            name: target.name.clone(),
            kind: target.kind.iter().map(ToString::to_string).collect(),
            crate_types: target
                .crate_types
                .iter()
                .map(ToString::to_string)
                .collect(),
            src_path: target.src_path.clone().into_std_path_buf(),
            edition: target.edition.to_string(),
            doc_capable: target.doc,
            doctest: target.doctest,
            test: target.test,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{
            Path,
            PathBuf,
        },
    };

    use cargo_metadata::MetadataCommand;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    use super::DocWorkspace;

    #[test]
    fn builds_doc_workspace_from_cargo_metadata_value() {
        let dir = temp_workspace();
        let metadata = cargo_metadata_for(dir.path());

        let workspace = DocWorkspace::from_cargo_metadata(metadata).unwrap();

        assert_eq!(workspace.workspace_root, dir.path().to_path_buf());
        assert_eq!(workspace.workspace_manifest_path, dir.path().join("Cargo.toml"));
        assert_eq!(workspace.packages.len(), 2);
        assert_eq!(workspace.packages[0].name, "alpha-crate");
        assert_eq!(workspace.packages[1].name, "beta-tool");

        let alpha = workspace.package("alpha-crate").unwrap();
        assert_eq!(alpha.package_root, dir.path().join("alpha"));
        assert_eq!(alpha.targets.len(), 1);
        assert_eq!(alpha.default_doc_target().unwrap().kind, vec!["lib"]);
        assert_eq!(
            alpha.default_doc_target().unwrap().rustdoc_output_stem(),
            "alpha_crate"
        );

        let beta = workspace.package("beta-tool").unwrap();
        assert_eq!(beta.package_root, dir.path().join("beta"));
        assert_eq!(beta.targets.len(), 1);
        assert_eq!(beta.default_doc_target().unwrap().kind, vec!["bin"]);
        assert_eq!(
            beta.default_doc_target().unwrap().rustdoc_output_stem(),
            "beta_tool"
        );
    }

    #[test]
    fn builds_doc_workspace_from_cargo_metadata_json() {
        let dir = temp_workspace();
        let metadata = cargo_metadata_for(dir.path());
        let json = serde_json::to_string(&metadata).unwrap();

        let workspace = DocWorkspace::from_cargo_metadata_json(&json).unwrap();

        assert_eq!(workspace.packages.len(), 2);
        assert_eq!(workspace.package("alpha-crate").unwrap().version, "0.1.0");
        assert_eq!(workspace.package("beta-tool").unwrap().version, "0.2.0");
    }

    #[test]
    fn builds_doc_workspace_from_cargo_metadata_file() {
        let dir = temp_workspace();
        let metadata = cargo_metadata_for(dir.path());
        let json = serde_json::to_string(&metadata).unwrap();
        let metadata_path = dir.path().join("target").join("metadata.json");
        fs::create_dir_all(metadata_path.parent().unwrap()).unwrap();
        fs::write(&metadata_path, json).unwrap();

        let workspace = DocWorkspace::from_cargo_metadata_file(&metadata_path).unwrap();

        assert_eq!(workspace.package("alpha-crate").unwrap().targets[0].edition, "2024");
        assert!(workspace
            .package("beta-tool")
            .unwrap()
            .targets[0]
            .doc_capable);
    }

    #[test]
    fn discovers_doc_workspace_from_repo_path() {
        let dir = temp_workspace();

        let workspace = DocWorkspace::discover_from(dir.path()).unwrap();

        assert_eq!(workspace.packages.len(), 2);
        assert_eq!(workspace.workspace_root, dir.path().to_path_buf());
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
description = "alpha"
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
description = "beta"
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

    #[allow(dead_code)]
    fn as_path_buf(path: &Path) -> PathBuf {
        path.to_path_buf()
    }
}
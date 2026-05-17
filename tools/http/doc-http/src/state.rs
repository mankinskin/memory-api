use std::{
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
};

use doc_api::DocWorkspace;

use crate::error::DocHttpError;

#[derive(Clone)]
pub struct DocAppState {
    repo_root: Arc<PathBuf>,
}

impl DocAppState {
    pub fn new(repo_root: PathBuf) -> Self {
        Self {
            repo_root: Arc::new(repo_root),
        }
    }

    pub fn repo_root(&self) -> &Path {
        self.repo_root.as_path()
    }

    pub fn load_workspace(&self) -> Result<DocWorkspace, DocHttpError> {
        Ok(DocWorkspace::discover_from(self.repo_root())?)
    }
}

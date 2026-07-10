pub mod error;
pub mod evidence;
pub mod outputs;
pub mod workspace;

pub use error::DocError;
pub use evidence::{
    DocEvidenceKind,
    DocEvidenceLinks,
    DocEvidenceRecord,
    DocEvidenceStatus,
};
pub use outputs::CargoDocArtifact;
pub use workspace::{
    DocPackage,
    DocTarget,
    DocWorkspace,
    DocWorkspaceSource,
};

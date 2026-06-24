//! Generic test-harness primitives shared across all domains.
//!
//! Gated behind the `testing` Cargo feature so the types are never compiled
//! into production builds.  Consumers add:
//!
//! ```toml
//! [dev-dependencies]
//! memory-api = { path = "...", features = ["testing"] }
//! ```
//!
//! # Design
//!
//! [`SandboxSetup`] is an inversion-of-control trait that each domain
//! implements to specify how its directory layout is created inside a fresh
//! [`TempDir`].  [`Sandbox<S>`] owns the [`TempDir`] and delegates all
//! path-related concerns to the domain-supplied `S`.
//!
//! The domain implementation (`impl SandboxSetup`) is also responsible for
//! running any initialisation commands (e.g. `ticket init`) inside
//! [`SandboxSetup::setup`].  That keeps the generic struct free of
//! domain-specific binary calls.

use std::path::{
    Path,
    PathBuf,
};

use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Paths produced by domain setup
// ---------------------------------------------------------------------------

/// Directory layout produced by a [`SandboxSetup`] implementation.
pub struct SandboxPaths {
    /// Directory that the CLI receives as `--index-root`.
    pub index_root: PathBuf,
    /// Root of the workspace (the parent project directory).
    ///
    /// For "flat" sandboxes this is equal to `index_root`.  For workspace
    /// sandboxes (e.g. when the index lives under `.ticket/`) this is the
    /// parent of that subdirectory.
    pub workspace_root: PathBuf,
}

// ---------------------------------------------------------------------------
// IoC trait — implemented once per domain
// ---------------------------------------------------------------------------

/// Domain-specific sandbox initialisation contract.
///
/// Implement this trait once per domain (e.g. `TicketFlatSetup`) and use it
/// as the type parameter for [`Sandbox<S>`].  The generic struct handles
/// temp-directory lifecycle; the implementation handles domain path layout and
/// any required store initialisation commands.
pub trait SandboxSetup {
    /// Called immediately after the [`TempDir`] is created.
    ///
    /// Implementations should:
    /// 1. Create any required subdirectories (e.g. `.ticket/`).
    /// 2. Run domain initialisation commands (e.g. `ticket init`).
    /// 3. Return the resulting [`SandboxPaths`].
    fn setup(workspace_root: &Path) -> SandboxPaths;
}

// ---------------------------------------------------------------------------
// Generic sandbox struct
// ---------------------------------------------------------------------------

/// An isolated, self-cleaning test sandbox backed by a [`TempDir`].
///
/// `S` is a [`SandboxSetup`] implementation that controls the directory layout
/// and store initialisation for the domain under test.  The `TempDir` is kept
/// alive for the sandbox's lifetime and deleted when it is dropped.
///
/// ```ignore
/// let s = Sandbox::<MyDomainSetup>::new();
/// // use s.index_root() / s.workspace_root() in commands
/// ```
pub struct Sandbox<S: SandboxSetup> {
    _dir: TempDir,
    paths: SandboxPaths,
    _marker: std::marker::PhantomData<fn() -> S>,
}

impl<S: SandboxSetup> Sandbox<S> {
    /// Create a new sandbox by calling [`SandboxSetup::setup`] on a fresh
    /// [`TempDir`].
    pub fn new() -> Self {
        let dir = TempDir::new().expect("failed to create sandbox temp dir");
        let paths = S::setup(dir.path());
        Self {
            _dir: dir,
            paths,
            _marker: std::marker::PhantomData,
        }
    }

    /// Path supplied as `--index-root` to CLI commands.
    pub fn index_root(&self) -> &Path {
        &self.paths.index_root
    }

    /// Root directory of the workspace (parent of the index root for
    /// workspace-layout sandboxes; equal to `index_root` for flat sandboxes).
    pub fn workspace_root(&self) -> &Path {
        &self.paths.workspace_root
    }
}

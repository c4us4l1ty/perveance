//! Descriptor-scoped Linux raster I/O building blocks, not execution authority.
//! No worker receives a host-directory capability. Input snapshots are sealed;
//! candidate artifacts must match trusted expected artwork before publication.
use serde::Serialize;

pub const PROJECT_FILE: &str = "output.perveance.json";
pub const PNG_FILE: &str = "output.png";
pub const MAX_RELATIVE_BYTES: usize = 1024;
pub const MAX_RELATIVE_COMPONENTS: usize = 8;

#[derive(Clone, Debug, Serialize)]
pub struct Artifact {
    pub file: &'static str,
    pub bytes: u64,
    pub sha256: String,
}

/// Atomic create-new publication of one private host-validated file. A false
/// final sync flag means it is already visible: never delete/blindly retry it.
#[derive(Debug, Serialize)]
pub struct FilePublication {
    pub file: String,
    pub bytes: u64,
    pub sha256: String,
    pub committed: bool,
    pub parent_directory_synced: bool,
    pub autonomous_execution: bool,
}

/// One atomically committed directory of fixed host-selected private files.
/// This is storage integrity/publication metadata, never execution authority.
#[derive(Debug, Serialize)]
pub struct DirectoryPublication {
    pub directory: String,
    pub artifacts: Vec<Artifact>,
    pub committed: bool,
    pub parent_directory_synced: bool,
    pub autonomous_execution: bool,
}

#[derive(Debug, Serialize)]
pub struct Publication {
    pub bundle: String,
    pub revision: String,
    pub artifacts: [Artifact; 2],
    pub committed: bool,
    /// False means the bundle is already visible, but parent sync failed.
    /// Inspect that same name; do not blindly retry or delete the published work.
    pub parent_directory_synced: bool,
    pub autonomous_execution: bool,
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{PrivateDirectory, ProjectSnapshot, VerifiedBundle};
#[cfg(target_os = "linux")]
pub(crate) use linux::{anonymous_file, sealed_input};

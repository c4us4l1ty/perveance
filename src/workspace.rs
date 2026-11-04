//! Experimental data-only project staging. This is not a sandbox, supervisor,
//! or authorization to run untrusted code. Only validated projects are staged;
//! arbitrary source directories, attachments and executables are not copied.

use crate::Project;
use std::path::{Path, PathBuf};

pub const STAGED_PROJECT_NAME: &str = "project.perveance.json";

/// Owns the temporary project snapshot. Dropping it attempts cleanup; use
/// `close` to report normal cleanup errors. It is not crash recovery.
pub struct StagedProject {
    directory: tempfile::TempDir,
    revision: String,
}

impl StagedProject {
    pub fn project_path(&self) -> PathBuf { self.directory.path().join(STAGED_PROJECT_NAME) }
    pub fn revision(&self) -> &str { &self.revision }

    pub fn close(self) -> Result<(), String> {
        self.directory.close().map_err(|e| format!("remove staged project: {e}"))
    }
}

/// Stage one already-loaded project under a caller-supplied **trusted** parent.
///
/// Linux only. The parent and process must not be controlled by an adversary:
/// this uses path-based filesystem APIs, not race-resistant traversal. The
/// snapshot includes all project content, including hidden pixels and names.
/// No model code runs, no worker gets access, and this is not a security gate.
#[cfg(target_os = "linux")]
pub fn stage_project(project: &Project, trusted_parent: &Path) -> Result<StagedProject, String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    project.validate()?;
    let revision = project.revision()?;
    let directory = tempfile::Builder::new().prefix("perveance-stage-")
        .permissions(std::fs::Permissions::from_mode(0o700)).tempdir_in(trusted_parent)
        .map_err(|e| format!("create staging directory: {e}"))?;
    let metadata = std::fs::symlink_metadata(directory.path()).map_err(|e| format!("inspect staging directory: {e}"))?;
    if !metadata.file_type().is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("staging directory is not private".into());
    }
    let path = directory.path().join(STAGED_PROJECT_NAME);
    project.save_new(&path)?;
    let metadata = std::fs::symlink_metadata(&path).map_err(|e| format!("inspect staged project: {e}"))?;
    if !metadata.file_type().is_file() || metadata.permissions().mode() & 0o077 != 0 || metadata.nlink() != 1 {
        return Err("staged project is not a private regular file".into());
    }
    let staged = Project::load(&path)?;
    if &staged != project || staged.revision()? != revision {
        return Err("staged project verification failed".into());
    }
    let mut entries = std::fs::read_dir(directory.path()).map_err(|e| format!("inspect staging contents: {e}"))?;
    let entry = entries.next().ok_or("staged project is missing")?
        .map_err(|e| format!("inspect staging contents: {e}"))?;
    if entry.file_name() != STAGED_PROJECT_NAME || entries.next().is_some() {
        return Err("unexpected staging contents".into());
    }
    Ok(StagedProject { directory, revision })
}

#[cfg(not(target_os = "linux"))]
pub fn stage_project(_project: &Project, _trusted_parent: &Path) -> Result<StagedProject, String> {
    Err("project staging is supported only on Linux".into())
}

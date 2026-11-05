use super::{Artifact, Publication, MAX_RELATIVE_BYTES, MAX_RELATIVE_COMPONENTS, PNG_FILE, PROJECT_FILE};
use crate::{Project, MAX_PNG_BYTES, MAX_PROJECT_BYTES};
use sha2::{Digest, Sha256};
use std::{
    ffi::CString,
    fs::{File, Metadata, OpenOptions},
    io::{Read, Write},
    os::{fd::{AsRawFd, FromRawFd, RawFd}, unix::{ffi::OsStrExt, fs::{FileExt, MetadataExt, OpenOptionsExt, PermissionsExt}}},
    path::Path,
};

// openat2 is mandatory. Never emulate these constraints with canonicalize/check
// followed by an ordinary open: that would reintroduce a pathname race.
const RESOLVE_NO_XDEV: u64 = 0x01;
const RESOLVE_NO_MAGICLINKS: u64 = 0x02;
const RESOLVE_NO_SYMLINKS: u64 = 0x04;
const RESOLVE_BENEATH: u64 = 0x08;
const BENEATH: u64 = RESOLVE_NO_XDEV | RESOLVE_NO_MAGICLINKS | RESOLVE_NO_SYMLINKS | RESOLVE_BENEATH;
const REQUIRED_SEALS: libc::c_int = libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL;
const STAGE_PREFIX: &str = ".perveance-publish-";

fn fail(context: &str) -> String { format!("worker I/O: {context}") }

#[repr(C)]
struct OpenHow { flags: u64, mode: u64, resolve: u64 }

fn open_at(directory: RawFd, name: &std::ffi::CStr, flags: libc::c_int, mode: u64, resolve: u64) -> Result<File, String> {
    let how = OpenHow { flags: flags as u64, mode, resolve };
    let fd = unsafe { libc::syscall(libc::SYS_openat2, directory, name.as_ptr(), &how, std::mem::size_of::<OpenHow>()) };
    if fd < 0 { return Err(fail("confined open failed; openat2 and the requested access are required, without fallback")); }
    Ok(unsafe { File::from_raw_fd(fd as RawFd) })
}

fn relative(value: &str) -> Result<CString, String> {
    if value.is_empty() || value.len() > MAX_RELATIVE_BYTES || value.bytes().any(|b| b.is_ascii_control() || b == b'\\' || b == b':') {
        return Err(fail("invalid relative artifact path"));
    }
    let parts: Vec<_> = value.split('/').collect();
    if parts.len() > MAX_RELATIVE_COMPONENTS || parts.iter().any(|part| part.is_empty() || *part == "." || *part == ".." || part.len() > 255) {
        return Err(fail("invalid relative artifact path"));
    }
    CString::new(value).map_err(|_| fail("invalid relative artifact path"))
}

fn bundle_name(value: &str) -> Result<CString, String> {
    if value.is_empty() || value.len() > 64 || value.starts_with('.')
        || !value.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)) {
        return Err(fail("bundle name must be a non-hidden 1 to 64 character ASCII basename"));
    }
    CString::new(value).map_err(|_| fail("invalid bundle basename"))
}

fn private_directory(file: &File) -> Result<(), String> {
    let metadata = file.metadata().map_err(|_| fail("cannot inspect directory capability"))?;
    if !metadata.is_dir() || metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o7777 != 0o700 {
        return Err(fail("directory must be owned by this user and mode 0700"));
    }
    Ok(())
}

fn same_inode(a: &Metadata, b: &Metadata) -> bool { a.dev() == b.dev() && a.ino() == b.ino() }
fn same_version(a: &Metadata, b: &Metadata) -> bool {
    same_inode(a, b) && a.len() == b.len() && a.mode() == b.mode() && a.uid() == b.uid() && a.gid() == b.gid()
        && a.nlink() == b.nlink() && a.mtime() == b.mtime() && a.mtime_nsec() == b.mtime_nsec()
        && a.ctime() == b.ctime() && a.ctime_nsec() == b.ctime_nsec()
}

fn private_regular(metadata: &Metadata, limit: u64) -> Result<(), String> {
    if !metadata.is_file() || metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o6077 != 0 || metadata.nlink() != 1 {
        return Err(fail("source must be a private owned regular file with one link"));
    }
    if metadata.len() > limit { return Err(fail("source exceeds its byte limit")); }
    Ok(())
}

// Only convert an already-held, type-checked inode descriptor into a readable
// description. The numeric path is generated internally, never from worker
// input. This deliberate trusted-procfs operation is NOT a user path resolver.
fn reopen_readonly(held: &File) -> Result<File, String> {
    let path = format!("/proc/self/fd/{}", held.as_raw_fd());
    let file = OpenOptions::new().read(true).custom_flags(libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path).map_err(|_| fail("cannot reopen held inode through trusted procfs"))?;
    if !same_inode(&held.metadata().map_err(|_| fail("cannot inspect held inode"))?,
                   &file.metadata().map_err(|_| fail("cannot inspect reopened inode"))?) {
        return Err(fail("held inode identity changed"));
    }
    Ok(file)
}

/// A host-side directory capability. Once opened, renaming/replacing its original
/// pathname does not redirect operations. Do not pass it to workers. The caller
/// must authorize the root object, filesystem and local account, not just a string.
pub struct PrivateDirectory { directory: File }
impl PrivateDirectory {
    pub fn open(path: &Path) -> Result<Self, String> {
        if unsafe { libc::geteuid() } == 0 { return Err(fail("use an unprivileged supervisor, never root")); }
        let bytes = path.as_os_str().as_bytes();
        if !path.is_absolute() || bytes.len() > 4096 || bytes.iter().any(|b| b.is_ascii_control())
            || bytes.split(|b| *b == b'/').skip(1).any(|part| part.is_empty() || part == b"." || part == b"..") {
            return Err(fail("root must be an absolute normal path without symlinks"));
        }
        let name = CString::new(bytes).map_err(|_| fail("invalid directory root"))?;
        let directory = open_at(libc::AT_FDCWD, &name, libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0, RESOLVE_NO_MAGICLINKS | RESOLVE_NO_SYMLINKS)?;
        private_directory(&directory)?;
        Ok(Self { directory })
    }

    fn child(&self, name: &str) -> Result<Self, String> {
        let name = relative(name)?;
        let directory = open_at(self.directory.as_raw_fd(), &name, libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0, BENEATH)?;
        private_directory(&directory)?;
        Ok(Self { directory })
    }

    fn read(&self, name: &str, limit: u64) -> Result<Vec<u8>, String> {
        self.read_with_hook(name, limit, || {})
    }

    // Trusted broker storage only. No directory descriptor or writable journal
    // is ever exported to an untrusted peer. Reuse the same mandatory resolver
    // and pre-activation inode checks as raster capture, rather than path checks.
    pub(crate) fn read_private(&self, name: &str, limit: u64) -> Result<Vec<u8>, String> {
        self.read(name, limit)
    }

    pub(crate) fn create_private(&self, name: &str) -> Result<File, String> {
        let name = relative(name)?;
        let file = open_at(self.directory.as_raw_fd(), &name,
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600, BENEATH)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| fail("cannot protect private storage file"))?;
        Ok(file)
    }

    pub(crate) fn open_private_rw(&self, name: &str, limit: u64) -> Result<File, String> {
        let name = relative(name)?;
        let held = open_at(self.directory.as_raw_fd(), &name,
            libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0, BENEATH)?;
        let before = held.metadata().map_err(|_| fail("cannot inspect private storage inode"))?;
        private_regular(&before, limit)?;
        let file = OpenOptions::new().read(true).write(true)
            .custom_flags(libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(format!("/proc/self/fd/{}", held.as_raw_fd()))
            .map_err(|_| fail("cannot reopen held private storage inode"))?;
        let after = file.metadata().map_err(|_| fail("cannot inspect private storage inode"))?;
        private_regular(&after, limit)?;
        if !same_inode(&before, &after) { return Err(fail("private storage inode identity changed")); }
        Ok(file)
    }

    pub(crate) fn sync(&self) -> Result<(), String> {
        self.directory.sync_all().map_err(|_| fail("cannot sync private directory"))
    }

    fn read_with_hook(&self, name: &str, limit: u64, hook: impl FnOnce()) -> Result<Vec<u8>, String> {
        let name = relative(name)?;
        // O_PATH does not activate a FIFO, socket or device. Reject their type
        // before any attempt to open them for reading; O_NONBLOCK alone is not
        // enough to prevent side effects of opening a special device.
        let held = open_at(self.directory.as_raw_fd(), &name, libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0, BENEATH)?;
        let before = held.metadata().map_err(|_| fail("cannot inspect source inode"))?;
        private_regular(&before, limit)?;
        hook();
        let file = reopen_readonly(&held)?;
        let mut bytes = Vec::new();
        (&file).take(limit + 1).read_to_end(&mut bytes).map_err(|_| fail("cannot read bounded source"))?;
        let after = file.metadata().map_err(|_| fail("cannot inspect consumed inode"))?;
        private_regular(&after, limit)?;
        if bytes.len() as u64 > limit || bytes.len() as u64 != before.len() || !same_version(&before, &after) {
            return Err(fail("source changed during capture or exceeded its limit"));
        }
        Ok(bytes)
    }
}

pub(crate) fn anonymous_file() -> Result<File, String> {
    let name = b"perveance-private\0";
    let fd = unsafe { libc::syscall(libc::SYS_memfd_create, name.as_ptr(), libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING) };
    if fd < 0 { return Err(fail("memfd_create is required, without disk fallback")); }
    let file = unsafe { File::from_raw_fd(fd as RawFd) };
    file.set_permissions(std::fs::Permissions::from_mode(0o600)).map_err(|_| fail("cannot protect anonymous file"))?;
    Ok(file)
}

fn has_required_seals(file: &File) -> bool {
    let seals = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GET_SEALS) };
    seals >= 0 && seals & REQUIRED_SEALS == REQUIRED_SEALS
}

struct FrozenFile { file: File, artifact: Artifact }
impl FrozenFile {
    fn new(bytes: &[u8], name: &'static str, limit: u64) -> Result<Self, String> {
        if bytes.len() as u64 > limit { return Err(fail("snapshot exceeds byte limit")); }
        let mut file = anonymous_file()?;
        file.write_all(bytes).map_err(|_| fail("cannot capture anonymous snapshot"))?;
        if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_ADD_SEALS, REQUIRED_SEALS) } != 0 {
            return Err(fail("cannot seal snapshot; no mutable fallback"));
        }
        if !has_required_seals(&file) {
            return Err(fail("snapshot seal verification failed"));
        }
        Ok(Self { file, artifact: Artifact { file: name, bytes: bytes.len() as u64, sha256: format!("{:x}", Sha256::digest(bytes)) } })
    }

    fn copy_to(&self, output: &mut File) -> Result<(), String> {
        let mut offset = 0;
        let mut buffer = [0u8; 64 * 1024];
        while offset < self.artifact.bytes {
            let remaining = (self.artifact.bytes - offset).min(buffer.len() as u64) as usize;
            let count = self.file.read_at(&mut buffer[..remaining], offset).map_err(|_| fail("cannot read sealed artifact"))?;
            if count == 0 { return Err(fail("incomplete sealed artifact")); }
            output.write_all(&buffer[..count]).map_err(|_| fail("cannot write private publication artifact"))?;
            offset += count as u64;
        }
        Ok(())
    }
}

/// A validated full raster snapshot, including hidden pixels/names. Bytes are
/// kernel-sealed, not merely called immutable. Does not authorize their disclosure
/// to a provider. Debug/Serialize deliberately omit the entire type.
pub struct ProjectSnapshot { project: Project, frozen: FrozenFile, revision: String }
impl ProjectSnapshot {
    pub fn capture(root: &PrivateDirectory, relative_project: &str) -> Result<Self, String> {
        let bytes = root.read(relative_project, MAX_PROJECT_BYTES)?;
        let project: Project = serde_json::from_slice(&bytes).map_err(|_| fail("invalid source project JSON or schema"))?;
        Self::from_project(&project)
    }

    pub fn from_project(project: &Project) -> Result<Self, String> {
        project.validate().map_err(|_| fail("invalid source project"))?;
        let bytes = serde_json::to_vec(project).map_err(|_| fail("cannot serialize source project"))?;
        let frozen = FrozenFile::new(&bytes, PROJECT_FILE, MAX_PROJECT_BYTES)?;
        let revision = project.revision().map_err(|_| fail("cannot fingerprint source project"))?;
        Ok(Self { project: project.clone(), frozen, revision })
    }

    pub fn project(&self) -> &Project { &self.project }
    pub fn revision(&self) -> &str { &self.revision }

    pub(crate) fn stdin_file(&self) -> Result<File, String> {
        // A new description starts at zero for every run. try_clone would share
        // an offset and silently break repeated/concurrent snapshot consumers.
        if !has_required_seals(&self.frozen.file) {
            return Err(fail("input snapshot lost required seals"));
        }
        reopen_readonly(&self.frozen.file)
    }
}

/// Canonical sealed artifacts. Construction requires either trusted rendering,
/// or exact comparison to trusted expected artwork. Candidate declarations of
/// success, revision, paths or tests do not establish this value.
pub struct VerifiedBundle { project: ProjectSnapshot, png: FrozenFile }
impl VerifiedBundle {
    pub fn render(project: &Project) -> Result<Self, String> {
        let snapshot = ProjectSnapshot::from_project(project)?;
        let bytes = project.png_bytes(1).map_err(|_| fail("cannot render expected project"))?;
        Ok(Self { project: snapshot, png: FrozenFile::new(&bytes, PNG_FILE, MAX_PNG_BYTES)? })
    }

    /// Call only after the candidate has stopped. `expected` must come from
    /// trusted policy/typed edits, never from a candidate-provided revision claim.
    pub fn capture(candidate: &PrivateDirectory, expected: &Project) -> Result<Self, String> {
        expected.validate().map_err(|_| fail("invalid trusted expected project"))?;
        let bytes = candidate.read(PROJECT_FILE, MAX_PROJECT_BYTES)?;
        let project: Project = serde_json::from_slice(&bytes).map_err(|_| fail("invalid candidate project JSON or schema"))?;
        project.validate().map_err(|_| fail("invalid candidate project"))?;
        if &project != expected { return Err(fail("candidate project differs from trusted expected artwork")); }
        let bytes = candidate.read(PNG_FILE, MAX_PNG_BYTES)?;
        let decoded = crate::decode_png_bytes(&bytes).map_err(|_| fail("invalid bounded candidate PNG"))?;
        if (decoded.width, decoded.height) != (expected.width, expected.height)
            || !decoded.pixels.iter().flatten().copied().eq(expected.rgba().map_err(|_| fail("cannot composite expected artwork"))?) {
            return Err(fail("candidate PNG differs from expected composite"));
        }
        // Re-encode from the trusted model. Never publish candidate-supplied PNG
        // metadata/text or noncanonical project bytes after validating pixels.
        Self::render(expected)
    }

    pub fn publish(&self, destination: &PrivateDirectory, name: &str) -> Result<Publication, String> {
        self.publish_with_hook(destination, name, |_| Ok(()))
    }

    fn publish_with_hook(&self, destination: &PrivateDirectory, name: &str,
                         mut hook: impl FnMut(Phase) -> Result<(), ()>) -> Result<Publication, String> {
        let target = bundle_name(name)?;
        let mut stage = Stage::create(destination)?;
        for (index, frozen) in [&self.project.frozen, &self.png].into_iter().enumerate() {
            let file_name = relative(frozen.artifact.file)?;
            let mut output = open_at(stage.directory.directory.as_raw_fd(), &file_name,
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0o600, BENEATH)?;
            frozen.copy_to(&mut output)?;
            output.sync_all().map_err(|_| fail("cannot sync staged artifact; bundle not published"))?;
            hook(Phase::FileSynced(index)).map_err(|_| fail("publication interrupted before commit"))?;
        }
        stage.directory.directory.sync_all().map_err(|_| fail("cannot sync staged bundle; bundle not published"))?;
        hook(Phase::BeforeCommit).map_err(|_| fail("publication interrupted before commit"))?;
        let staged_name = CString::new(stage.name.as_bytes()).map_err(|_| fail("invalid private stage name"))?;
        if unsafe { libc::syscall(libc::SYS_renameat2, destination.directory.as_raw_fd(), staged_name.as_ptr(),
            destination.directory.as_raw_fd(), target.as_ptr(), libc::RENAME_NOREPLACE) } != 0 {
            return Err(fail("cannot install new bundle; occupied names are never replaced and renameat2 is required"));
        }
        stage.committed = true;
        // The directory is already installed. Never return a pre-commit-looking
        // error or remove it if the final sync fails: that would invite data loss.
        let parent_directory_synced = hook(Phase::Committed).is_ok() && destination.directory.sync_all().is_ok();
        Ok(Publication { bundle: name.to_owned(), revision: self.project.revision.clone(),
            artifacts: [self.project.frozen.artifact.clone(), self.png.artifact.clone()],
            committed: true, parent_directory_synced, autonomous_execution: false })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase { FileSynced(usize), BeforeCommit, Committed }

struct Stage<'a> { parent: &'a PrivateDirectory, directory: PrivateDirectory, name: String, committed: bool }
impl<'a> Stage<'a> {
    fn create(parent: &'a PrivateDirectory) -> Result<Self, String> {
        let mut bytes = [0u8; 16];
        if unsafe { libc::getrandom(bytes.as_mut_ptr().cast(), bytes.len(), 0) } != bytes.len() as isize {
            return Err(fail("cannot allocate unpredictable publication stage"));
        }
        let name = format!("{STAGE_PREFIX}{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());
        let c_name = CString::new(name.as_bytes()).map_err(|_| fail("invalid private stage name"))?;
        if unsafe { libc::mkdirat(parent.directory.as_raw_fd(), c_name.as_ptr(), 0o700) } != 0 {
            return Err(fail("cannot create private publication stage"));
        }
        match parent.child(&name) {
            Ok(directory) => Ok(Self { parent, directory, name, committed: false }),
            Err(error) => {
                unsafe { libc::unlinkat(parent.directory.as_raw_fd(), c_name.as_ptr(), libc::AT_REMOVEDIR); }
                Err(error)
            }
        }
    }
}
impl Drop for Stage<'_> {
    fn drop(&mut self) {
        if self.committed { return; }
        // Only fixed files in this held private stage; never recurse through a
        // candidate directory or use an original destination pathname again.
        for name in [PROJECT_FILE, PNG_FILE] {
            if let Ok(name) = CString::new(name) { unsafe { libc::unlinkat(self.directory.directory.as_raw_fd(), name.as_ptr(), 0); } }
        }
        let ours = self.directory.directory.metadata();
        if let (Ok(ours), Ok(current)) = (ours, self.parent.child(&self.name)) {
            if current.directory.metadata().is_ok_and(|value| same_inode(&ours, &value)) {
                if let Ok(name) = CString::new(self.name.as_bytes()) {
                    unsafe { libc::unlinkat(self.parent.directory.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR); }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;

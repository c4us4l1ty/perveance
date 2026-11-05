use super::*;
use sha2::{Digest, Sha256};
use std::{fs::{self, File, OpenOptions}, io::{Cursor, Read}, os::{fd::AsRawFd, unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt}}, path::PathBuf};

const MANIFEST: &str = "task.json";
const LOCK: &str = "lock";
const CANCEL: &str = "cancel.requested";
const CANCEL_BYTES: &[u8] = b"cancel\n";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    id: String,
    kind: String,
    base_revision: String,
    expected_revision: String,
    proposal_sha256: String,
}

struct Job {
    directory: PathBuf,
    manifest: Manifest,
    history: Vec<TaskEvent>,
}

fn valid_id(id: &str) -> bool {
    id.strip_prefix("task-").is_some_and(|suffix| (6..=64).contains(&suffix.len()) && suffix.bytes().all(|b| b.is_ascii_alphanumeric()))
}
fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn valid_digest(text: &str) -> bool { text.len() == 64 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
fn valid_revision(text: &str) -> bool { text.strip_prefix("sha256-json-v1:").is_some_and(valid_digest) }

fn private_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "cannot inspect task directory")?;
    if !metadata.file_type().is_dir() || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o077 != 0 {
        return Err("task directories must be owned by the current user, private (0700), and not symlinks".into());
    }
    Ok(())
}

// O_NONBLOCK prevents opening a planted FIFO from hanging; O_NOFOLLOW rejects
// final-component symlinks. Ancestors and same-user processes must be trusted.
fn private_file(path: &Path, writable: bool) -> Result<File, String> {
    let file = OpenOptions::new().read(true).write(writable)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC).open(path)
        .map_err(|_| "cannot open private task file")?;
    let metadata = file.metadata().map_err(|_| "cannot inspect task file")?;
    if !metadata.file_type().is_file() || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o077 != 0 || metadata.nlink() != 1 {
        return Err("task file must be a private, owned regular file with one link".into());
    }
    Ok(file)
}
fn read_private(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let file = private_file(path, false)?;
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).map_err(|_| "cannot read task file")?;
    if bytes.len() as u64 > limit { return Err("task file exceeds size limit".into()); }
    Ok(bytes)
}
fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    serde_json::from_slice(&read_private(path, MAX_METADATA_BYTES)?).map_err(|_| "invalid task metadata JSON or schema".into())
}
fn sync_directory(path: &Path) -> Result<(), String> {
    File::open(path).and_then(|file| file.sync_all()).map_err(|_| "cannot sync task directory".into())
}
fn write_json<T: Serialize>(directory: &Path, name: &str, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "cannot serialize task metadata")?;
    if bytes.len() as u64 > MAX_METADATA_BYTES { return Err("task metadata exceeds size limit".into()); }
    crate::write_new(&directory.join(name), &bytes).map_err(|_| "cannot create task metadata")?;
    sync_directory(directory)
}
fn event_name(sequence: usize) -> String { format!("event-{sequence:03}.json") }
fn exists(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("cannot inspect task path".into()),
    }
}

/// Advisory, nonblocking cooperative lock. Closing this descriptor releases
/// it, including on process exit. This is not exclusion from a hostile user.
fn try_lock(directory: &Path) -> Result<Option<File>, String> {
    let file = private_file(&directory.join(LOCK), true)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 { return Ok(Some(file)); }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::EWOULDBLOCK) { Ok(None) }
    else { Err("task locking is unavailable".into()) }
}

fn validate_event(event: &TaskEvent, previous: Option<TaskState>, sequence: usize) -> Result<(), String> {
    let transition = matches!((previous, event.state),
        (None, TaskState::Queued) | (Some(TaskState::Queued), TaskState::Running | TaskState::Cancelled)
        | (Some(TaskState::Running), TaskState::Succeeded | TaskState::Failed | TaskState::Cancelled | TaskState::Interrupted));
    let reason = match event.state {
        TaskState::Queued | TaskState::Running | TaskState::Succeeded => event.reason.is_none(),
        TaskState::Cancelled => event.reason == Some(TaskReason::UserCancelled),
        TaskState::Interrupted => event.reason == Some(TaskReason::WorkerLost),
        TaskState::Failed => matches!(event.reason, Some(TaskReason::InvalidInputs | TaskReason::InputsChanged
            | TaskReason::OutputWriteFailed | TaskReason::OutputVerificationFailed)),
    };
    if event.version != TASK_VERSION || event.sequence != sequence || !transition || !reason
        || (event.state == TaskState::Succeeded) != event.outputs.is_some() {
        return Err("invalid task event or transition".into());
    }
    if let Some(outputs) = &event.outputs {
        if outputs.project.file != OUTPUT_PROJECT || outputs.png.file != OUTPUT_PNG
            || outputs.project.bytes == 0 || outputs.project.bytes > crate::MAX_PROJECT_BYTES
            || outputs.png.bytes == 0 || outputs.png.bytes > crate::MAX_PNG_BYTES
            || !valid_digest(&outputs.project.sha256) || !valid_digest(&outputs.png.sha256) {
            return Err("invalid task artifact metadata".into());
        }
    }
    Ok(())
}

fn load(root: &Path, id: &str) -> Result<Job, String> {
    if !valid_id(id) { return Err("invalid task ID".into()); }
    private_directory(root)?;
    let directory = root.join(id);
    private_directory(&directory)?;
    // Bound directory inspection and reject extra/future journal entries rather
    // than pretending that a truncated history is the full task state.
    for (index, entry) in fs::read_dir(&directory).map_err(|_| "cannot list task directory")?.enumerate() {
        if index >= 64 { return Err("task directory has too many entries".into()); }
        let name = entry.map_err(|_| "cannot inspect task directory entry")?.file_name();
        if let Some(name) = name.to_str() {
            if name.starts_with("event-") && !(0..MAX_EVENTS).any(|seq| name == event_name(seq)) {
                return Err("unsupported task journal entry".into());
            }
        }
    }
    let manifest: Manifest = read_json(&directory.join(MANIFEST))?;
    if manifest.version != TASK_VERSION || manifest.id != id || manifest.kind != "raster_proposal"
        || !valid_revision(&manifest.base_revision) || !valid_revision(&manifest.expected_revision)
        || !valid_digest(&manifest.proposal_sha256) {
        return Err("invalid task manifest".into());
    }
    let mut history: Vec<TaskEvent> = Vec::new();
    let mut missing = false;
    for sequence in 0..MAX_EVENTS {
        let path = directory.join(event_name(sequence));
        if !exists(&path)? { missing = true; continue; }
        if missing { return Err("task journal contains a gap".into()); }
        let event: TaskEvent = read_json(&path)?;
        validate_event(&event, history.last().map(|previous| previous.state), sequence)?;
        history.push(event);
    }
    if history.is_empty() { return Err("task journal is missing".into()); }
    Ok(Job { directory, manifest, history })
}

impl Job {
    fn state(&self) -> TaskState { self.history.last().expect("validated nonempty history").state }
    fn append(&mut self, state: TaskState, reason: Option<TaskReason>, outputs: Option<TaskOutputs>) -> Result<(), String> {
        if self.history.len() >= MAX_EVENTS { return Err("task journal limit reached".into()); }
        let event = TaskEvent { version: TASK_VERSION, sequence: self.history.len(), state, reason, outputs };
        validate_event(&event, Some(self.state()), event.sequence)?;
        write_json(&self.directory, &event_name(event.sequence), &event)?;
        self.history.push(event);
        Ok(())
    }
    fn cancelled(&self) -> Result<bool, String> {
        let path = self.directory.join(CANCEL);
        if !exists(&path)? { return Ok(false); }
        if read_private(&path, CANCEL_BYTES.len() as u64)? != CANCEL_BYTES { return Err("invalid task cancellation marker".into()); }
        Ok(true)
    }
    fn report(self) -> Result<TaskReport, String> {
        let cancel_requested = self.cancelled()?;
        let last = self.history.last().expect("validated nonempty history");
        let outputs = last.outputs.clone();
        if let Some(outputs) = &outputs { verify_outputs(&self.directory, &self.manifest, outputs)?; }
        Ok(TaskReport { version: TASK_VERSION, id: self.manifest.id, kind: "raster_proposal", state: last.state,
            sequence: last.sequence, base_revision: self.manifest.base_revision, expected_revision: self.manifest.expected_revision,
            cancel_requested, artifacts_verified: outputs.is_some(), outputs, history: self.history,
            autonomous_execution: false, network_used: false, resource_quotas_enforced: false })
    }
}

/// Snapshot and preflight a revision-bound proposal into a new private task.
/// Original paths are not retained or reread by the runner. Submission preflights
/// in memory but starts no job and writes no result artwork. Only the compiled
/// raster model is supported.
pub fn create(root: &Path, project: &Project, proposal: &EditProposal) -> Result<TaskReport, String> {
    private_directory(root)?;
    let mut candidate = project.clone();
    candidate.apply_proposal(proposal).map_err(|_| "invalid, stale or unsupported raster task proposal")?;
    let proposal_bytes = serde_json::to_vec(proposal).map_err(|_| "cannot serialize task proposal")?;
    if proposal_bytes.len() as u64 > crate::MAX_PROPOSAL_BYTES { return Err("task proposal exceeds size limit".into()); }
    let directory = tempfile::Builder::new().prefix("task-").rand_bytes(16)
        .permissions(fs::Permissions::from_mode(0o700)).tempdir_in(root).map_err(|_| "cannot create private task directory")?;
    let id = directory.path().file_name().and_then(|s| s.to_str()).ok_or("invalid generated task ID")?.to_owned();
    if !valid_id(&id) { return Err("invalid generated task ID".into()); }
    let manifest = Manifest { version: TASK_VERSION, id: id.clone(), kind: "raster_proposal".into(),
        base_revision: project.revision()?, expected_revision: candidate.revision()?, proposal_sha256: digest(&proposal_bytes) };
    project.save_new(&directory.path().join(INPUT_PROJECT)).map_err(|_| "cannot snapshot task project")?;
    crate::write_new(&directory.path().join(INPUT_PROPOSAL), &proposal_bytes).map_err(|_| "cannot snapshot task proposal")?;
    crate::write_new(&directory.path().join(LOCK), b"").map_err(|_| "cannot create task lock")?;
    write_json(directory.path(), MANIFEST, &manifest)?
    let queued = TaskEvent { version: TASK_VERSION, sequence: 0, state: TaskState::Queued, reason: None, outputs: None };
    write_json(directory.path(), &event_name(0), &queued)?;
    let _ = directory.keep();
    // Each file and the task directory are synced before the parent's new entry.
    // Hardware/filesystem behavior can still defeat durability; this is not a
    // general power-loss guarantee or a global storage quota.
    sync_directory(root)?;
    inspect(root, &id)
}

pub fn inspect(root: &Path, id: &str) -> Result<TaskReport, String> { load(root, id)?.report() }

fn artifact(directory: &Path, name: &str, limit: u64) -> Result<Artifact, String> {
    let bytes = read_private(&directory.join(name), limit)?;
    Ok(Artifact { file: name.into(), bytes: bytes.len() as u64, sha256: digest(&bytes) })
}
fn parse_project(bytes: &[u8]) -> Result<Project, String> {
    let project: Project = serde_json::from_slice(bytes).map_err(|_| "invalid task project")?;
    project.validate().map_err(|_| "invalid task project")?;
    Ok(project)
}
fn verify_outputs(directory: &Path, manifest: &Manifest, outputs: &TaskOutputs) -> Result<(), String> {
    let project_bytes = read_private(&directory.join(OUTPUT_PROJECT), crate::MAX_PROJECT_BYTES)?;
    let png_bytes = read_private(&directory.join(OUTPUT_PNG), crate::MAX_PNG_BYTES)?;
    if project_bytes.len() as u64 != outputs.project.bytes || digest(&project_bytes) != outputs.project.sha256
        || png_bytes.len() as u64 != outputs.png.bytes || digest(&png_bytes) != outputs.png.sha256 {
        return Err("task output integrity check failed".into());
    }
    let project = parse_project(&project_bytes)?;
    if project.revision()? != manifest.expected_revision { return Err("task result revision mismatch".into()); }
    let mut decoder = png::Decoder::new(Cursor::new(png_bytes));
    decoder.set_limits(png::Limits { bytes: crate::MAX_PNG_BYTES as usize });
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    let mut reader = decoder.read_info().map_err(|_| "invalid task PNG")?;
    let info = reader.info();
    if (info.width, info.height) != (project.width, project.height) || info.color_type != png::ColorType::Rgba
        || info.bit_depth != png::BitDepth::Eight || info.animation_control.is_some()
        || reader.output_buffer_size() != (project.width * project.height * 4) as usize {
        return Err("task PNG does not match project".into());
    }
    let mut pixels = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut pixels).map_err(|_| "invalid task PNG pixels")?;
    let expected = project.rgba()?;
    if &pixels[..frame.buffer_size()] != expected.as_slice() { return Err("task PNG pixels do not match project".into()); }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase { Running, Applied, ProjectWritten, PngWritten }
enum RunOutcome { Succeeded(TaskOutputs), Cancelled }
fn check_cancelled(job: &Job) -> Result<bool, TaskReason> { job.cancelled().map_err(|_| TaskReason::InvalidInputs) }

/// Holds the cooperative lock throughout the fixed, bounded raster pipeline.
/// Cancellation is checked between stages, not within a synchronous model edit
/// or file write. No OS CPU/memory/disk quota or hard deadline is claimed.
pub fn run(root: &Path, id: &str) -> Result<TaskReport, String> { run_inner(root, id, |_| {}) }
fn run_inner(root: &Path, id: &str, mut checkpoint: impl FnMut(Phase)) -> Result<TaskReport, String> {
    let initial = load(root, id)?;
    let _lock = try_lock(&initial.directory)?.ok_or("task is busy")?;
    let mut job = load(root, id)?;
    if job.state() == TaskState::Succeeded { return job.report(); }
    if job.state() == TaskState::Running { return Err("task needs explicit interrupted-run recovery; it will not be replayed".into()); }
    if job.state() != TaskState::Queued { return Err("task is terminal and cannot be replayed; submit a new task".into()); }
    if job.cancelled()? {
        job.append(TaskState::Cancelled, Some(TaskReason::UserCancelled), None)?;
        return job.report();
    }
    job.append(TaskState::Running, None, None)?;
    checkpoint(Phase::Running);
    let result = (|| -> Result<RunOutcome, TaskReason> {
        if check_cancelled(&job)? { return Ok(RunOutcome::Cancelled); }
        let project_bytes = read_private(&job.directory.join(INPUT_PROJECT), crate::MAX_PROJECT_BYTES).map_err(|_| TaskReason::InvalidInputs)?;
        let proposal_bytes = read_private(&job.directory.join(INPUT_PROPOSAL), crate::MAX_PROPOSAL_BYTES).map_err(|_| TaskReason::InvalidInputs)?;
        if digest(&proposal_bytes) != job.manifest.proposal_sha256 { return Err(TaskReason::InputsChanged); }
        let mut project = parse_project(&project_bytes).map_err(|_| TaskReason::InvalidInputs)?;
        if project.revision().map_err(|_| TaskReason::InvalidInputs)? != job.manifest.base_revision { return Err(TaskReason::InputsChanged); }
        let proposal: EditProposal = serde_json::from_slice(&proposal_bytes).map_err(|_| TaskReason::InvalidInputs)?;
        project.apply_proposal(&proposal).map_err(|_| TaskReason::InvalidInputs)?;
        if project.revision().map_err(|_| TaskReason::InvalidInputs)? != job.manifest.expected_revision { return Err(TaskReason::InputsChanged); }
        checkpoint(Phase::Applied);
        if check_cancelled(&job)? { return Ok(RunOutcome::Cancelled); }
        project.save_new(&job.directory.join(OUTPUT_PROJECT)).map_err(|_| TaskReason::OutputWriteFailed)?;
        checkpoint(Phase::ProjectWritten);
        if check_cancelled(&job)? { return Ok(RunOutcome::Cancelled); }
        project.export_png_new(&job.directory.join(OUTPUT_PNG)).map_err(|_| TaskReason::OutputWriteFailed)?;
        checkpoint(Phase::PngWritten);
        if check_cancelled(&job)? { return Ok(RunOutcome::Cancelled); }
        let outputs = TaskOutputs { project: artifact(&job.directory, OUTPUT_PROJECT, crate::MAX_PROJECT_BYTES).map_err(|_| TaskReason::OutputVerificationFailed)?,
            png: artifact(&job.directory, OUTPUT_PNG, crate::MAX_PNG_BYTES).map_err(|_| TaskReason::OutputVerificationFailed)? };
        verify_outputs(&job.directory, &job.manifest, &outputs).map_err(|_| TaskReason::OutputVerificationFailed)?;
        if check_cancelled(&job)? { return Ok(RunOutcome::Cancelled); }
        Ok(RunOutcome::Succeeded(outputs))
    })();
    match result {
        Ok(RunOutcome::Succeeded(outputs)) => job.append(TaskState::Succeeded, None, Some(outputs))?,
        Ok(RunOutcome::Cancelled) => job.append(TaskState::Cancelled, Some(TaskReason::UserCancelled), None)?,
        Err(reason) => {
            job.append(TaskState::Failed, Some(reason), None)?;
            return Err("raster task failed; inspect its history for the failure category".into());
        }
    }
    job.report()
}

/// A durable cooperative request. Cancellation is not a process kill. A late
/// request can lose to successful completion; inspect the returned task state.
pub fn cancel(root: &Path, id: &str) -> Result<TaskReport, String> {
    let initial = load(root, id)?;
    if initial.state().is_terminal() { return initial.report(); }
    if !initial.cancelled()? {
        let marker = initial.directory.join(CANCEL);
        if crate::write_new(&marker, CANCEL_BYTES).is_err() && !initial.cancelled()? { return Err("cannot record task cancellation".into()); }
        sync_directory(&initial.directory)?;
    }
    if let Some(_lock) = try_lock(&initial.directory)? {
        let mut job = load(root, id)?;
        if !job.state().is_terminal() { job.append(TaskState::Cancelled, Some(TaskReason::UserCancelled), None)?; }
        return job.report();
    }
    inspect(root, id)
}

/// Never resumes or replays an interrupted task. A running journal entry can
/// be marked interrupted only after obtaining the same exclusive worker lock.
/// Unpublished outputs stay in the private directory for explicit inspection.
pub fn recover(root: &Path, id: &str) -> Result<TaskReport, String> {
    let initial = load(root, id)?;
    let _lock = try_lock(&initial.directory)?.ok_or("task is busy; active work cannot be recovered")?;
    let mut job = load(root, id)?;
    match job.state() {
        TaskState::Running => job.append(TaskState::Interrupted, Some(TaskReason::WorkerLost), None)?,
        TaskState::Queued if job.cancelled()? => job.append(TaskState::Cancelled, Some(TaskReason::UserCancelled), None)?,
        _ => {}
    }
    job.report()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{panic::{catch_unwind, AssertUnwindSafe}, sync::mpsc, time::Duration};

    fn root() -> tempfile::TempDir {
        tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o700)).tempdir().unwrap()
    }
    fn queued(root: &Path) -> String {
        let project = Project::new(4, 4).unwrap();
        let proposal = EditProposal { version: 1, base_revision: project.revision().unwrap(),
            operations: vec![crate::Operation::Pixel { layer: 0, x: 2, y: 3, color: [7, 8, 9, 128] }] };
        create(root, &project, &proposal).unwrap().id
    }

    #[test]
    fn cancellation_is_cooperative_at_each_stage_and_cannot_recover_a_live_runner() {
        for pause in [Phase::Running, Phase::Applied, Phase::ProjectWritten, Phase::PngWritten] {
            let root = root();
            let id = queued(root.path());
            let (reached_tx, reached_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let worker = {
                let path = root.path().to_owned();
                let id = id.clone();
                std::thread::spawn(move || run_inner(&path, &id, |phase| {
                    if phase == pause {
                        reached_tx.send(()).unwrap();
                        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                    }
                }))
            };
            reached_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            let report = cancel(root.path(), &id).unwrap();
            assert_eq!(report.state, TaskState::Running);
            assert!(report.cancel_requested);
            assert!(report.outputs.is_none());
            assert!(recover(root.path(), &id).is_err());
            assert!(run(root.path(), &id).is_err());
            release_tx.send(()).unwrap();
            let report = worker.join().unwrap().unwrap();
            assert_eq!(report.state, TaskState::Cancelled, "{pause:?}");
            assert_eq!(report.history.len(), 3);
            assert!(report.outputs.is_none());
            assert!(!report.artifacts_verified);
            let directory = root.path().join(&id);
            assert_eq!(directory.join(OUTPUT_PROJECT).exists(), matches!(pause, Phase::ProjectWritten | Phase::PngWritten));
            assert_eq!(directory.join(OUTPUT_PNG).exists(), pause == Phase::PngWritten);
            assert_eq!(recover(root.path(), &id).unwrap().state, TaskState::Cancelled);
            assert!(run(root.path(), &id).is_err());
        }
    }

    #[test]
    fn interrupted_stages_are_never_replayed_or_advertised_as_success() {
        for stop in [Phase::Running, Phase::Applied, Phase::ProjectWritten, Phase::PngWritten] {
            let root = root();
            let id = queued(root.path());
            // Inject abnormal unwinding at each pipeline boundary. This is a
            // deterministic crash simulation, not a power-loss certification.
            let result = catch_unwind(AssertUnwindSafe(|| run_inner(root.path(), &id, |phase| {
                if phase == stop { panic!("injected interruption"); }
            })));
            assert!(result.is_err());
            let report = inspect(root.path(), &id).unwrap();
            assert_eq!(report.state, TaskState::Running);
            assert!(report.outputs.is_none());
            assert!(run(root.path(), &id).is_err());
            let recovered = recover(root.path(), &id).unwrap();
            assert_eq!(recovered.state, TaskState::Interrupted);
            assert_eq!(recovered.history[2].reason, Some(TaskReason::WorkerLost));
            assert!(recovered.outputs.is_none());
            assert!(!recovered.artifacts_verified);
            assert_eq!(recover(root.path(), &id).unwrap().history, recovered.history);
            assert!(run(root.path(), &id).is_err());
        }
    }

    #[test]
    fn malformed_future_oversized_or_gapped_journals_fail_closed() {
        let root = root();
        for bad in [
            br#"{"version":2,"sequence":1,"state":"running","reason":null,"outputs":null}"#.as_slice(),
            br#"{"version":1,"sequence":2,"state":"running","reason":null,"outputs":null}"#,
            br#"{"version":1,"sequence":1,"state":"succeeded","reason":null,"outputs":null}"#,
            br#"{"version":1,"sequence":1,"state":"running","reason":null,"outputs":null,"private-secret":1}"#,
            br#"{"version":1,"sequence":1,"state":"cancelled","reason":"worker_lost","outputs":null}"#,
        ] {
            let id = queued(root.path());
            let directory = root.path().join(&id);
            crate::write_new(&directory.join(event_name(1)), bad).unwrap();
            let error = inspect(root.path(), &id).unwrap_err();
            assert!(!error.contains("private-secret"));
            assert!(run(root.path(), &id).is_err());
            assert!(cancel(root.path(), &id).is_err());
            assert!(recover(root.path(), &id).is_err());
        }
        let id = queued(root.path());
        let directory = root.path().join(&id);
        let running = TaskEvent { version: 1, sequence: 2, state: TaskState::Running, reason: None, outputs: None };
        write_json(&directory, &event_name(2), &running).unwrap();
        assert!(inspect(root.path(), &id).unwrap_err().contains("gap"));
        fs::remove_file(directory.join(event_name(2))).unwrap();
        crate::write_new(&directory.join("event-003.json"), b"{}").unwrap();
        assert!(inspect(root.path(), &id).is_err());
        fs::remove_file(directory.join("event-003.json")).unwrap();
        crate::write_new(&directory.join(event_name(1)), &vec![b' '; MAX_METADATA_BYTES as usize + 1]).unwrap();
        assert!(inspect(root.path(), &id).is_err());
    }

    #[test]
    fn forged_artifact_paths_and_cancel_markers_are_not_followed() {
        let root = root();
        let id = queued(root.path());
        let mut job = load(root.path(), &id).unwrap();
        job.append(TaskState::Running, None, None).unwrap();
        let outputs = TaskOutputs {
            project: Artifact { file: "../../escape".into(), bytes: 1, sha256: "0".repeat(64) },
            png: Artifact { file: OUTPUT_PNG.into(), bytes: 1, sha256: "0".repeat(64) },
        };
        assert!(job.append(TaskState::Succeeded, None, Some(outputs)).is_err());
        assert_eq!(inspect(root.path(), &id).unwrap().state, TaskState::Running);
        let id = queued(root.path());
        let directory = root.path().join(&id);
        std::os::unix::fs::symlink(INPUT_PROJECT, directory.join(CANCEL)).unwrap();
        assert!(cancel(root.path(), &id).is_err());
        assert!(run(root.path(), &id).is_err());
        fs::remove_file(directory.join(CANCEL)).unwrap();
        crate::write_new(&directory.join(CANCEL), b"not cancellation").unwrap();
        assert!(inspect(root.path(), &id).is_err());
    }

    #[test]
    fn pending_cancel_after_interrupted_acknowledgement_is_recoverable() {
        let root = root();
        let id = queued(root.path());
        let directory = root.path().join(&id);
        crate::write_new(&directory.join(CANCEL), CANCEL_BYTES).unwrap();
        let recovered = recover(root.path(), &id).unwrap();
        assert_eq!(recovered.state, TaskState::Cancelled);
        assert!(recovered.cancel_requested);
        assert_eq!(recovered.history.len(), 2);
    }
}

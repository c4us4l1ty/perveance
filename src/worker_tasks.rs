//! Durable, fixed pixel-worker jobs. This is NOT an autonomous coding harness.
//! Captured typed pixels, one synthetic offline broker scope, actual kernel
//! quotas and independent whole-project verification precede atomic publication.
use crate::{bounded_worker::{self, BackendIdentity, BrokerProgress, BrokerWorkerReport, Limits, Outcome},
    broker::{Ledger, LedgerReport, Policy}, worker_io::{Artifact, PrivateDirectory, ProjectSnapshot, VerifiedBundle},
    EditProposal, Operation, Project};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Write, os::fd::AsRawFd, path::Path,
    sync::atomic::{AtomicBool, Ordering}, thread, time::Duration};

const JOURNAL: &str = "worker-task.v1";
const LOCK: &str = "worker-task.lock";
const INPUT: &str = "input.perveance.json";
const PROPOSAL: &str = "proposal.json";
const CANCEL: &str = "cancel.requested";
const RESULT: &str = "result";
const META_LIMIT: u64 = 16 * 1024;
const RECORD_LIMIT: usize = 3072;
const KIND: &str = "fixed_pixels_offline_v1";

fn fail(text: &str) -> String { format!("worker task: {text}") }
fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn valid_hash(value: &str) -> bool { value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
fn valid_revision(value: &str) -> bool { value.strip_prefix("sha256-json-v1:").is_some_and(valid_hash) }
fn valid_id(id: &str) -> bool {
    id.strip_prefix("worker-").is_some_and(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State { Queued, Running, Prepared, Succeeded, Failed, Cancelled, Interrupted }
impl State { pub fn terminal(self) -> bool { !matches!(self, Self::Queued | Self::Running | Self::Prepared) } }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason { UserCancelled, InputsChanged, WorkerStopped, CandidateRejected, PublicationFailed, OwnerLost }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Execution {
    pub worker_started: bool,
    pub cgroup_limits_applied: bool,
    pub cgroup_removed: bool,
    pub input_sealed: bool,
    pub worker_completed: bool,
    pub boundary_checks: bool,
    pub adapter_invocations: u64,
    pub reserved_calls: u64,
    pub reserved_cost_microusd: u64,
    pub ledger_halted: bool,
}
impl Execution {
    fn from_report(report: &BrokerWorkerReport) -> Self {
        Self { worker_started: report.worker.worker_started, cgroup_limits_applied: report.worker.cgroup_limits_applied,
            cgroup_removed: report.worker.cgroup_removed, input_sealed: report.worker.input_snapshot_sealed,
            worker_completed: report.worker.outcome == Outcome::Completed, boundary_checks: report.namespace_checks_verified,
            adapter_invocations: report.adapter_invocations, reserved_calls: report.ledger.reserved_calls,
            reserved_cost_microusd: report.ledger.reserved_cost_microusd, ledger_halted: report.ledger.halted }
    }
    fn verified(&self) -> bool {
        self.worker_started && self.cgroup_limits_applied && self.cgroup_removed && self.input_sealed
            && self.worker_completed && self.boundary_checks && self.adapter_invocations == 1
            && self.reserved_calls == 1 && self.reserved_cost_microusd == 10 && !self.ledger_halted
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub sequence: usize, pub state: State, pub reason: Option<Reason>,
    pub backend: Option<BackendIdentity>, pub execution: Option<Execution>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    version: u32, id: String, kind: String, base_revision: String, expected_revision: String,
    input_sha256: String, proposal_sha256: String, limits: Limits,
}
#[derive(Debug, Serialize)]
pub struct Outputs { pub bundle: &'static str, pub revision: String, pub artifacts: [Artifact; 2] }
#[derive(Debug, Serialize)]
pub struct Report {
    pub version: u32, pub id: String, pub kind: &'static str, pub state: State,
    pub base_revision: String, pub expected_revision: String, pub limits: Limits,
    pub history: Vec<Event>, pub cancel_requested: bool, pub outputs: Option<Outputs>,
    pub artifacts_verified: bool, pub ledger: Option<LedgerReport>,
    pub accounting_status_unknown: bool, pub resource_quotas_enforced: bool,
    pub synthetic_fixture_only: bool, pub provider_network_used: bool,
    pub autonomous_execution: bool, pub production_security_verified: bool,
}
#[derive(Debug, Serialize)]
pub struct Cancellation { pub id: String, pub request_recorded: bool, pub already_terminal: bool }
struct Job { directory: PrivateDirectory, header: Header, history: Vec<Event> }

pub(crate) fn validate_pixels(project: &Project, proposal: &EditProposal) -> Result<Project, String> {
    if proposal.operations.is_empty() || proposal.operations.len() > crate::MAX_OPERATIONS
        || !proposal.operations.iter().all(|op| matches!(op, Operation::Pixel { .. })) {
        return Err(fail("only 1 to 1024 captured pixel operations are supported"));
    }
    let mut expected = project.clone();
    expected.apply_proposal(proposal).map_err(|_| fail("invalid or stale pixel proposal"))?;
    Ok(expected)
}
fn write(directory: &PrivateDirectory, name: &str, bytes: &[u8]) -> Result<(), String> {
    let mut file = directory.create_private(name)?;
    file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|_| fail("cannot durably write private task data"))?;
    directory.sync()
}
fn encoded<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec(value).map_err(|_| fail("cannot encode task record"))?;
    if bytes.len() > RECORD_LIMIT { return Err(fail("task record exceeds limit")); }
    bytes.push(b'\n'); Ok(bytes)
}
fn valid_event(event: &Event, previous: Option<&Event>, sequence: usize) -> bool {
    let transition = match previous.map(|p| p.state) {
        None => event.state == State::Queued,
        Some(State::Queued) => matches!(event.state, State::Running | State::Cancelled),
        Some(State::Running) => matches!(event.state, State::Prepared | State::Failed | State::Cancelled | State::Interrupted),
        Some(State::Prepared) => matches!(event.state, State::Succeeded | State::Failed | State::Cancelled | State::Interrupted),
        _ => false,
    };
    let reason = match event.state {
        State::Queued | State::Running | State::Prepared | State::Succeeded => event.reason.is_none(),
        State::Cancelled => event.reason == Some(Reason::UserCancelled),
        State::Interrupted => event.reason == Some(Reason::OwnerLost),
        State::Failed => matches!(event.reason, Some(Reason::InputsChanged | Reason::WorkerStopped | Reason::CandidateRejected | Reason::PublicationFailed)),
    };
    let initial = matches!(event.state, State::Queued) || (event.state == State::Cancelled && sequence == 1);
    let backend = if initial { event.backend.is_none() && event.execution.is_none() }
        else { event.backend.is_some() && previous.is_none_or(|p| p.backend.is_none() || event.backend == p.backend) };
    let verified = !matches!(event.state, State::Prepared | State::Succeeded)
        || event.execution.as_ref().is_some_and(Execution::verified);
    let retained = previous.is_none_or(|p| p.state != State::Prepared || p.execution == event.execution);
    transition && reason && backend && verified && retained && event.sequence == sequence
        && sequence < 4 && (event.state != State::Running || event.execution.is_none())
}
fn child(root: &Path, id: &str) -> Result<PrivateDirectory, String> {
    if !valid_id(id) { return Err(fail("invalid job ID")); }
    PrivateDirectory::open(root)?.child(id)
}
fn load(directory: PrivateDirectory, id: &str) -> Result<Job, String> {
    let bytes = directory.read_private(JOURNAL, META_LIMIT)?;
    if bytes.last() != Some(&b'\n') { return Err(fail("partial journal; never truncate or retry execution")); }
    let lines: Vec<_> = bytes[..bytes.len()-1].split(|b| *b == b'\n').collect();
    if !(2..=5).contains(&lines.len()) || lines.iter().any(|line| line.len() > RECORD_LIMIT) {
        return Err(fail("invalid bounded journal"));
    }
    let header: Header = serde_json::from_slice(lines[0]).map_err(|_| fail("invalid task header"))?;
    header.limits.validate()?;
    if header.version != 1 || header.kind != KIND || header.id != id || !valid_id(id)
        || !valid_revision(&header.base_revision) || !valid_revision(&header.expected_revision)
        || !valid_hash(&header.input_sha256) || !valid_hash(&header.proposal_sha256) {
        return Err(fail("unsupported task binding"));
    }
    let mut history = Vec::new();
    for (sequence, line) in lines[1..].iter().enumerate() {
        let event: Event = serde_json::from_slice(line).map_err(|_| fail("invalid task event"))?;
        if !valid_event(&event, history.last(), sequence) { return Err(fail("invalid task transition or evidence")); }
        history.push(event);
    }
    Ok(Job { directory, header, history })
}
fn lock(directory: &PrivateDirectory) -> Result<Option<File>, String> {
    let file = directory.open_private_rw(LOCK, 0)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 { return Ok(Some(file)); }
    if std::io::Error::last_os_error().raw_os_error() == Some(libc::EWOULDBLOCK) { return Ok(None); }
    Err(fail("task locking unavailable"))
}
impl Job {
    fn last(&self) -> &Event { self.history.last().expect("validated history") }
    fn state(&self) -> State { self.last().state }
    fn cancelled(&self) -> Result<bool, String> {
        if !self.directory.exists(CANCEL)? { return Ok(false); }
        if self.directory.read_private(CANCEL, 7)? != b"cancel\n" { return Err(fail("invalid cancellation marker")); }
        Ok(true)
    }
    fn append(&mut self, state: State, reason: Option<Reason>, backend: Option<BackendIdentity>, execution: Option<Execution>) -> Result<(), String> {
        let event = Event { sequence: self.history.len(), state, reason, backend, execution };
        if !valid_event(&event, Some(self.last()), event.sequence) { return Err(fail("invalid task append")); }
        let bytes = encoded(&event)?;
        let mut file = self.directory.open_private_rw(JOURNAL, META_LIMIT)?;
        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::End(0)).map_err(|_| fail("cannot seek task journal"))?;
        file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|_| fail("task accounting uncertain; explicit inspection required"))?;
        self.directory.sync()?;
        self.history.push(event); Ok(())
    }
    fn inputs(&self) -> Result<(ProjectSnapshot, EditProposal, Project), String> {
        let input = self.directory.read_private(INPUT, crate::MAX_PROJECT_BYTES)?;
        let proposal = self.directory.read_private(PROPOSAL, crate::MAX_PROPOSAL_BYTES)?;
        if digest(&input) != self.header.input_sha256 || digest(&proposal) != self.header.proposal_sha256 {
            return Err(fail("captured inputs changed"));
        }
        let project: Project = serde_json::from_slice(&input).map_err(|_| fail("invalid captured project"))?;
        let proposal: EditProposal = serde_json::from_slice(&proposal).map_err(|_| fail("invalid captured proposal"))?;
        let snapshot = ProjectSnapshot::from_project(&project)?;
        let expected = validate_pixels(&project, &proposal)?;
        if snapshot.revision() != self.header.base_revision || expected.revision().map_err(|_| fail("invalid expected project"))? != self.header.expected_revision {
            return Err(fail("captured revision mismatch"));
        }
        Ok((snapshot, proposal, expected))
    }
    fn ledger(&self) -> Result<Ledger, String> {
        let ledger = Ledger::open(self.directory.child("ledger")?)?;
        if ledger.report().policy != Policy::fixture(1, 10, 10) { return Err(fail("immutable fixture policy changed")); }
        Ok(ledger)
    }
    fn outputs(&self) -> Result<Outputs, String> {
        let (_, _, expected) = self.inputs()?;
        let directory = self.directory.child(RESULT)?;
        let bundle = VerifiedBundle::capture(&directory, &expected)?;
        // Report hashes only when the actual stored bytes are canonical too.
        let actual_project = directory.read_private(crate::worker_io::PROJECT_FILE, crate::MAX_PROJECT_BYTES)?;
        let actual_png = directory.read_private(crate::worker_io::PNG_FILE, crate::MAX_PNG_BYTES)?;
        let artifacts = bundle.artifacts();
        if digest(&actual_project) != artifacts[0].sha256 || digest(&actual_png) != artifacts[1].sha256 {
            return Err(fail("published bytes differ from verified canonical artifacts"));
        }
        Ok(Outputs { bundle: RESULT, revision: self.header.expected_revision.clone(), artifacts })
    }
    fn report(self) -> Result<Report, String> {
        let cancel_requested = self.cancelled()?;
        let state = self.state();
        let outputs = if state == State::Succeeded {
            // An earlier append/sync may have returned an uncertain error even
            // though complete bytes are visible. Establish durability now;
            // visibility alone is not a retroactive successful sync receipt.
            self.directory.open_private_rw(JOURNAL, META_LIMIT)?.sync_all()
                .map_err(|_| fail("terminal journal durability remains uncertain"))?;
            self.directory.sync()?;
            Some(self.outputs()?)
        } else { None };
        let ledger = match self.ledger() {
            Ok(ledger) => Some(ledger.report()),
            Err(_) if state == State::Running => None, // active broker may own lock; unknown is NOT spare authority
            Err(error) => return Err(error),
        };
        if state == State::Succeeded && !ledger.as_ref().is_some_and(|r| !r.halted && r.calls.len() == 1 && r.calls[0].state == crate::broker::CallState::Completed
            && r.reserved_calls == 1 && r.reserved_cost_microusd == 10) { return Err(fail("completed ledger evidence changed")); }
        let quotas = self.last().execution.as_ref().is_some_and(|e| e.cgroup_limits_applied);
        Ok(Report { version: 1, id: self.header.id, kind: KIND, state, base_revision: self.header.base_revision,
            expected_revision: self.header.expected_revision, limits: self.header.limits, history: self.history,
            cancel_requested, artifacts_verified: outputs.is_some(), outputs, accounting_status_unknown: ledger.is_none(), ledger,
            resource_quotas_enforced: quotas, synthetic_fixture_only: true, provider_network_used: false,
            autonomous_execution: false, production_security_verified: false })
    }
}

/// No original paths are saved. An incomplete create may leave a private,
/// unannounced directory; it is not queued/executable until its journal validates.
pub fn create(root: &Path, project: &Project, proposal: &EditProposal, limits: &Limits) -> Result<Report, String> {
    limits.validate()?;
    let snapshot = ProjectSnapshot::from_project(project)?;
    let expected = validate_pixels(project, proposal)?;
    let input = serde_json::to_vec(project).map_err(|_| fail("cannot capture input"))?;
    let proposal = serde_json::to_vec(proposal).map_err(|_| fail("cannot capture proposal"))?;
    if proposal.len() as u64 > crate::MAX_PROPOSAL_BYTES { return Err(fail("proposal exceeds limit")); }
    let mut random = [0u8; 16];
    if unsafe { libc::getrandom(random.as_mut_ptr().cast(), random.len(), 0) } != random.len() as isize { return Err(fail("job identity unavailable")); }
    let id = format!("worker-{}", random.iter().map(|b| format!("{b:02x}")).collect::<String>());
    let parent = PrivateDirectory::open(root)?;
    let directory = parent.create_child(&id)?;
    write(&directory, INPUT, &input)?; write(&directory, PROPOSAL, &proposal)?; write(&directory, LOCK, b"")?;
    let ledger = directory.create_child("ledger")?;
    drop(Ledger::create(ledger, Policy::fixture(1, 10, 10))?);
    let header = Header { version: 1, id: id.clone(), kind: KIND.into(), base_revision: snapshot.revision().into(),
        expected_revision: expected.revision().map_err(|_| fail("cannot fingerprint expected project"))?,
        input_sha256: digest(&input), proposal_sha256: digest(&proposal), limits: limits.clone() };
    let event = Event { sequence: 0, state: State::Queued, reason: None, backend: None, execution: None };
    let mut journal = encoded(&header)?; journal.extend(encoded(&event)?);
    write(&directory, JOURNAL, &journal)?; parent.sync()?;
    load(directory, &id)?.report()
}
pub fn create_from_files(input_root: &Path, relative_project: &str, proposal_root: &Path, relative_proposal: &str,
    task_root: &Path, limits: &Limits) -> Result<Report, String> {
    let snapshot = ProjectSnapshot::capture(&PrivateDirectory::open(input_root)?, relative_project)?;
    let bytes = PrivateDirectory::open(proposal_root)?.read_private(relative_proposal, crate::MAX_PROPOSAL_BYTES)?;
    let proposal: EditProposal = serde_json::from_slice(&bytes).map_err(|_| fail("invalid proposal JSON or schema"))?;
    create(task_root, snapshot.project(), &proposal, limits)
}
pub fn inspect(root: &Path, id: &str) -> Result<Report, String> { load(child(root, id)?, id)?.report() }

pub fn cancel(root: &Path, id: &str) -> Result<Cancellation, String> {
    let directory = child(root, id)?;
    let guard = lock(&directory)?;
    if guard.is_some() && load(directory.try_clone()?, id)?.state().terminal() {
        return Ok(Cancellation { id: id.into(), request_recorded: false, already_terminal: true });
    }
    if write(&directory, CANCEL, b"cancel\n").is_err() && directory.read_private(CANCEL, 7)? != b"cancel\n" {
        return Err(fail("cannot record cancellation"));
    }
    directory.sync()?;
    if guard.is_some() {
        let mut job = load(directory, id)?;
        if job.state() == State::Queued { job.append(State::Cancelled, Some(Reason::UserCancelled), None, None)?; }
    }
    Ok(Cancellation { id: id.into(), request_recorded: true, already_terminal: false })
}

struct Stop<'a>(&'a AtomicBool);
impl Drop for Stop<'_> { fn drop(&mut self) { self.0.store(true, Ordering::SeqCst); } }

pub fn run(root: &Path, id: &str, delegated_root: &Path) -> Result<Report, String> {
    run_with_hook(root, id, delegated_root, bounded_worker::BrokerMode::Inspect, &BrokerProgress::default(), |_| {})
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase { Running, WorkerStopped, Prepared, Published }
fn run_with_hook(root: &Path, id: &str, delegated_root: &Path, mode: bounded_worker::BrokerMode,
    progress: &BrokerProgress, mut checkpoint: impl FnMut(Phase)) -> Result<Report, String> {
    let directory = child(root, id)?;
    let _lock = lock(&directory)?.ok_or_else(|| fail("job is busy"))?;
    let mut job = load(directory, id)?;
    if job.state() == State::Succeeded { return job.report(); }
    if job.state() != State::Queued { return Err(fail("job cannot be replayed; explicit recovery required")); }
    if job.cancelled()? { job.append(State::Cancelled, Some(Reason::UserCancelled), None, None)?; return job.report(); }
    // Missing/occupied backends do not turn queued work into host execution.
    let backend = bounded_worker::backend_identity(delegated_root)?;
    job.append(State::Running, None, Some(backend.clone()), None)?;
    checkpoint(Phase::Running);
    let inputs = job.inputs();
    let (snapshot, proposal, expected) = match inputs {
        Ok(inputs) => inputs,
        Err(_) => { job.append(State::Failed, Some(Reason::InputsChanged), Some(backend), None)?; return job.report(); }
    };
    if job.directory.exists(RESULT)? {
        job.append(State::Failed, Some(Reason::PublicationFailed), Some(backend), None)?; return job.report();
    }
    let ledger = job.ledger()?;
    let cancel = AtomicBool::new(false); let stop = AtomicBool::new(false); let invalid_cancel = AtomicBool::new(false);
    let result = thread::scope(|scope| {
        let stop_guard = Stop(&stop);
        scope.spawn(|| {
            while !stop.load(Ordering::SeqCst) {
                match job.cancelled() {
                    Ok(false) => {}, Ok(true) => { cancel.store(true, Ordering::SeqCst); break; },
                    Err(_) => { invalid_cancel.store(true, Ordering::SeqCst); cancel.store(true, Ordering::SeqCst); break; }
                }
                thread::sleep(Duration::from_millis(10));
            }
        });
        let result = bounded_worker::run_pixel_fixture(&snapshot, &proposal, delegated_root, ledger,
            &root.join(id).join("ledger"), &backend, mode, &job.header.limits, &cancel, progress);
        drop(stop_guard); result
    });
    checkpoint(Phase::WorkerStopped);
    let (report, candidate) = match result {
        Ok(result) => result,
        Err(_) => return Err(fail("worker/broker stopped without terminal evidence; task remains running for explicit whole-backend recovery")),
    };
    let execution = Some(Execution::from_report(&report));
    if invalid_cancel.load(Ordering::SeqCst) {
        job.append(State::Failed, Some(Reason::InputsChanged), Some(backend), execution)?; return job.report();
    }
    if report.worker.outcome == Outcome::Cancelled || job.cancelled()? {
        job.append(State::Cancelled, Some(Reason::UserCancelled), Some(backend), execution)?; return job.report();
    }
    let bundle = if execution.as_ref().is_some_and(Execution::verified) {
        candidate.as_ref().and_then(|bytes| VerifiedBundle::from_candidate_project(bytes, &expected).ok())
    } else { None };
    let Some(bundle) = bundle else {
        job.append(State::Failed, Some(Reason::CandidateRejected), Some(backend), execution)?; return job.report();
    };
    job.append(State::Prepared, None, Some(backend.clone()), execution.clone())?;
    checkpoint(Phase::Prepared);
    if job.cancelled()? {
        job.append(State::Cancelled, Some(Reason::UserCancelled), Some(backend), execution)?; return job.report();
    }
    let publication = match bundle.publish(&job.directory, RESULT) {
        Ok(publication) => publication,
        Err(_) => { job.append(State::Failed, Some(Reason::PublicationFailed), Some(backend), execution)?; return job.report(); }
    };
    checkpoint(Phase::Published);
    if !publication.parent_directory_synced {
        return Err(fail("bundle already visible; task remains prepared; inspect/recover without retrying or deleting"));
    }
    job.append(State::Succeeded, None, Some(backend), execution)?;
    job.report()
}

/// Explicit whole-backend orphan cleanup; requires the exact recorded backend.
/// Never reruns a worker/provider or clears/refunds the separate durable ledger.
pub fn recover(root: &Path, id: &str, delegated_root: &Path) -> Result<Report, String> {
    let directory = child(root, id)?;
    let _lock = lock(&directory)?.ok_or_else(|| fail("active job cannot be recovered"))?;
    let mut job = load(directory, id)?;
    if job.state().terminal() { return job.report(); }
    if job.state() == State::Queued {
        if job.cancelled()? { job.append(State::Cancelled, Some(Reason::UserCancelled), None, None)?; }
        return job.report();
    }
    let backend = bounded_worker::backend_identity(delegated_root)?;
    if job.last().backend.as_ref() != Some(&backend) { return Err(fail("recovery backend identity mismatch")); }
    bounded_worker::recover(delegated_root)?;
    let execution = job.last().execution.clone();
    if job.state() == State::Prepared && job.directory.exists(RESULT)? {
        job.outputs()?; job.directory.sync()?;
        job.append(State::Succeeded, None, Some(backend), execution)?;
    } else {
        job.append(State::Interrupted, Some(Reason::OwnerLost), Some(backend), execution)?;
    }
    job.report()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, process::{Child, Command, Stdio}, sync::mpsc, time::Instant};
    fn root() -> tempfile::TempDir { tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o700)).tempdir().unwrap() }
    fn backend() -> std::path::PathBuf { std::env::var_os("PERVEANCE_TEST_CGROUP_ROOT").expect("delegated kernel test root required").into() }
    fn queued(root: &Path) -> String {
        let project = Project::new(64, 64).unwrap();
        let proposal = EditProposal { version: 1, base_revision: project.revision().unwrap(),
            operations: vec![Operation::Pixel { layer: 0, x: 1, y: 2, color: [7, 8, 9, 128] }] };
        create(root, &project, &proposal, &Limits::default()).unwrap().id
    }
    fn wait_for(mut ready: impl FnMut() -> bool) {
        let start = Instant::now();
        while !ready() { assert!(start.elapsed() < Duration::from_secs(8), "durable worker task test timed out"); thread::sleep(Duration::from_millis(10)); }
    }
    #[test]
    #[ignore = "requires the real delegated cgroup/bubblewrap backend"]
    fn durable_cancellation_revokes_active_broker_and_prepared_publication_without_replay() {
        let backend = backend();
        for active in [true, false] {
            let root = root(); let id = queued(root.path());
            let progress = BrokerProgress::default();
            let (ready_tx, ready_rx) = mpsc::channel(); let (release_tx, release_rx) = mpsc::channel();
            thread::scope(|scope| {
                let path = root.path(); let id_ref = &id; let backend_ref = &backend; let progress_ref = &progress;
                let worker = scope.spawn(move || run_with_hook(path, id_ref, backend_ref,
                    if active { bounded_worker::BrokerMode::HoldInFlight } else { bounded_worker::BrokerMode::Inspect }, progress_ref, |phase| {
                        if !active && phase == Phase::Prepared { ready_tx.send(()).unwrap(); release_rx.recv_timeout(Duration::from_secs(8)).unwrap(); }
                    }));
                if active { wait_for(|| progress.phase() == 2); } else { ready_rx.recv_timeout(Duration::from_secs(8)).unwrap(); }
                assert!(recover(root.path(), &id, &backend).is_err());
                assert!(run(root.path(), &id, &backend).is_err());
                assert!(cancel(root.path(), &id).unwrap().request_recorded);
                if !active { release_tx.send(()).unwrap(); }
                let report = worker.join().unwrap().unwrap();
                assert_eq!(report.state, State::Cancelled);
                assert!(report.outputs.is_none() && report.resource_quotas_enforced);
                let ledger = report.ledger.unwrap();
                assert_eq!((ledger.reserved_calls, ledger.reserved_cost_microusd), (1, 10));
                assert_eq!(ledger.halted, active);
                if active { assert_eq!(ledger.calls[0].state, crate::broker::CallState::OutcomeUnknown); }
            });
            assert!(!root.path().join(&id).join(RESULT).exists());
            assert!(run(root.path(), &id, &backend).is_err());
            assert_eq!(recover(root.path(), &id, &backend).unwrap().state, State::Cancelled);
        }
    }

    // Process-only test harness; no application environment switches/commands.
    #[test]
    fn owner_helper() {
        let Some(root) = std::env::var_os("PERVEANCE_WORKER_TASK_TEST_ROOT") else { return; };
        let root = std::path::PathBuf::from(root);
        let id = std::env::var("PERVEANCE_WORKER_TASK_TEST_ID").unwrap();
        let target = std::env::var("PERVEANCE_WORKER_TASK_TEST_STOP").unwrap();
        run_with_hook(&root, &id, &backend(), bounded_worker::BrokerMode::Inspect, &BrokerProgress::default(), |phase| {
            if format!("{phase:?}") == target {
                fs::write(root.join("paused"), b"verified fixed checkpoint").unwrap();
                loop { thread::sleep(Duration::from_secs(60)); }
            }
        }).unwrap();
    }
    struct Owner(Child);
    impl Drop for Owner { fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); } }
    #[test]
    #[ignore = "requires the real delegated cgroup/bubblewrap backend"]
    fn killed_owner_never_replays_and_recovers_only_an_already_committed_verified_bundle() {
        let backend = backend();
        for phase in ["Prepared", "Published"] {
            let root = root(); let id = queued(root.path());
            let mut owner = Owner(Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "worker_tasks::tests::owner_helper", "--test-threads=1", "--nocapture"])
                .env("PERVEANCE_WORKER_TASK_TEST_ROOT", root.path()).env("PERVEANCE_WORKER_TASK_TEST_ID", &id)
                .env("PERVEANCE_WORKER_TASK_TEST_STOP", phase)
                .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::inherit()).spawn().unwrap());
            wait_for(|| root.path().join("paused").exists());
            owner.0.kill().unwrap(); owner.0.wait().unwrap();
            let report = inspect(root.path(), &id).unwrap(); assert_eq!(report.state, State::Prepared);
            assert!(report.outputs.is_none());
            assert!(run(root.path(), &id, &backend).is_err());
            let wrong = root.path().join("not-a-backend");
            assert!(recover(root.path(), &id, &wrong).is_err());
            let recovered = recover(root.path(), &id, &backend).unwrap();
            assert_eq!(recovered.state, if phase == "Published" { State::Succeeded } else { State::Interrupted });
            assert_eq!(recovered.artifacts_verified, phase == "Published");
            let ledger = recovered.ledger.unwrap();
            assert_eq!((ledger.reserved_calls, ledger.reserved_cost_microusd), (1, 10));
            assert_eq!(ledger.calls[0].state, crate::broker::CallState::Completed);
            let report = recover(root.path(), &id, &backend).unwrap();
            assert_eq!(report.ledger.unwrap().reserved_calls, 1);
            if phase == "Published" { assert_eq!(run(root.path(), &id, &backend).unwrap().state, State::Succeeded); }
            else { assert!(run(root.path(), &id, &backend).is_err()); }
        }
    }
}

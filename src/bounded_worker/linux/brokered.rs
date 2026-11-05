//! Fixed offline integration, not a general worker/broker executor.
use super::*;
use crate::{broker::{self, AdmissionPhase, Broker, Credential, FixtureTransport, Ledger, ProviderCall,
    Receipt, ReplyStatus, Route, ServeReport, Transport, TransportFailure},
    bounded_worker::{BrokerMode, BrokerProgress, BrokerWorkerReport}};
use std::{net::{TcpListener, Shutdown}, os::{fd::OwnedFd, unix::net::{UnixListener, UnixStream}},
    sync::atomic::AtomicU64};

const SCRIPT: &str = include_str!("../broker_probe.py");
const EXCHANGES: usize = 5;

pub(super) struct Channel<'a> {
    pub(super) ready: &'a AtomicBool,
    pub(super) revoked: &'a AtomicBool,
    endpoint: Option<UnixStream>,
    config: String,
    input: Option<File>,
    pub(super) expected_backend: Option<crate::bounded_worker::BackendIdentity>,
}
impl Channel<'_> {
    pub(super) fn configure(&mut self, command: &mut Command) -> Result<(), String> {
        let socket = self.endpoint.take().ok_or_else(|| fail("broker endpoint already transferred"))?;
        command.arg(SCRIPT).arg(&self.config).stdout(Stdio::from(OwnedFd::from(socket)));
        if let Some(input) = self.input.take() { command.stdin(Stdio::from(input)); }
        Ok(())
    }
}

// EOF/half-close revokes authority even when a complete request is buffered.
// This is a connected capability, not SO_PEERCRED authentication or a listener.
fn peer_live(peer: &UnixStream) -> bool {
    let mut poll = libc::pollfd { fd: peer.as_raw_fd(), events: libc::POLLIN | libc::POLLRDHUP, revents: 0 };
    if unsafe { libc::poll(&mut poll, 1, 0) } < 0
        || poll.revents & (libc::POLLHUP | libc::POLLRDHUP | libc::POLLERR | libc::POLLNVAL) != 0 { return false; }
    let mut byte = 0u8;
    let count = unsafe { libc::recv(peer.as_raw_fd(), (&mut byte as *mut u8).cast(), 1, libc::MSG_PEEK | libc::MSG_DONTWAIT) };
    count > 0 || (count < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::WouldBlock)
}
struct Scope<'a> {
    cancelled: &'a AtomicBool,
    revoked: &'a AtomicBool,
    deadline: Instant,
    peer: &'a UnixStream,
}
impl Scope<'_> {
    fn live(&self) -> bool {
        !self.cancelled.load(Ordering::SeqCst) && !self.revoked.load(Ordering::SeqCst)
            && Instant::now() < self.deadline && peer_live(self.peer)
    }
}
struct Revoke<'a>(&'a AtomicBool);
impl Drop for Revoke<'_> { fn drop(&mut self) { self.0.store(true, Ordering::SeqCst); } }

struct Offline<'a> { scope: &'a Scope<'a>, mode: BrokerMode, progress: &'a BrokerProgress, invocations: &'a AtomicU64 }
impl Transport for Offline<'_> {
    fn route(&self) -> Route { Route::fixture() }
    fn complete(&mut self, key: &Credential, call: ProviderCall<'_>) -> Result<Receipt, TransportFailure> {
        self.invocations.fetch_add(1, Ordering::SeqCst);
        self.progress.state.store(2, Ordering::SeqCst);
        if self.mode == BrokerMode::HoldInFlight {
            while self.scope.live() { thread::sleep(Duration::from_millis(10)); }
            return Err(TransportFailure::OutcomeUnknown);
        }
        if !self.scope.live() { return Err(TransportFailure::OutcomeUnknown); }
        FixtureTransport.complete(key, call)
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Summary {
    width: u32, height: u32, layers: usize,
    private_files_denied: bool, host_socket_denied: bool, host_tcp_denied: bool,
    host_proc_denied: bool, descriptors_clean: bool, environment_clean: bool,
    input_sealed: bool, exchanges_verified: bool,
}
pub(super) fn failure(output: &mut File) -> String {
    // Fixed allowlist only. Never propagate peer stderr, paths, text or keys.
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Failure { probe_failed: bool, stage: String }
    let mut bytes = Vec::new();
    if output.seek(SeekFrom::Start(0)).is_ok() && output.take(OUTPUT_LIMIT + 1).read_to_end(&mut bytes).is_ok()
        && bytes.len() as u64 <= OUTPUT_LIMIT {
        if let Ok(value) = serde_json::from_slice::<Failure>(&bytes) {
            if value.probe_failed && ["configuration", "descriptors", "channel", "environment", "input", "private-files",
                "symlink", "proc", "host-listeners", "project", "broker"].contains(&value.stage.as_str()) {
                return fail(&format!("fixed broker peer failed at {}", value.stage));
            }
        }
    }
    fail("fixed broker peer failed without a valid stage code")
}

pub(super) fn verify(output: &mut File, project: &Project) -> Result<Outcome, String> {
    output.seek(SeekFrom::Start(0)).map_err(|_| fail("cannot read broker-worker summary"))?;
    let mut bytes = Vec::new();
    output.take(OUTPUT_LIMIT + 1).read_to_end(&mut bytes).map_err(|_| fail("cannot read broker-worker summary"))?;
    let summary: Summary = serde_json::from_slice(&bytes).map_err(|_| fail("invalid broker-worker summary"))?;
    if bytes.len() as u64 > OUTPUT_LIMIT || (summary.width, summary.height, summary.layers)
        != (project.width, project.height, project.layers.len()) || !summary.private_files_denied
        || !summary.host_socket_denied || !summary.host_tcp_denied || !summary.host_proc_denied
        || !summary.descriptors_clean || !summary.environment_clean || !summary.input_sealed || !summary.exchanges_verified {
        return Err(fail("broker-worker confinement/protocol checks failed"));
    }
    Ok(Outcome::Completed)
}

pub(in crate::bounded_worker) fn run(snapshot: &ProjectSnapshot, delegated_root: &Path, ledger_root: &Path,
                                     mode: BrokerMode, limits: &Limits, cancelled: &AtomicBool,
                                     progress: &BrokerProgress) -> Result<BrokerWorkerReport, String> {
    let ledger = Ledger::open(PrivateDirectory::open(ledger_root)?)?;
    run_captured(snapshot, delegated_root, ledger_root, ledger, mode, limits, cancelled, progress, None)
        .map(|(report, _)| report)
}

pub(in crate::bounded_worker) fn run_pixels(snapshot: &ProjectSnapshot, proposal: &crate::EditProposal,
    delegated_root: &Path, ledger: Ledger, ledger_path_hint: &Path, backend: &crate::bounded_worker::BackendIdentity, mode: BrokerMode, limits: &Limits,
    cancelled: &AtomicBool, progress: &BrokerProgress) -> Result<(BrokerWorkerReport, Option<Vec<u8>>), String> {
    crate::worker_tasks::validate_pixels(snapshot.project(), proposal)?;
    #[derive(serde::Serialize)]
    struct Input<'a> { version: u32, project: &'a Project, proposal: &'a crate::EditProposal }
    let bytes = serde_json::to_vec(&Input { version: 1, project: snapshot.project(), proposal })
        .map_err(|_| fail("cannot capture fixed pixel task input"))?;
    let input = crate::worker_io::sealed_input(&bytes, crate::MAX_PROJECT_BYTES + crate::MAX_PROPOSAL_BYTES + 128)?;
    run_captured(snapshot, delegated_root, ledger_path_hint, ledger, mode, limits, cancelled, progress, Some((input, backend.clone())))
}

fn run_captured(snapshot: &ProjectSnapshot, delegated_root: &Path, ledger_root: &Path, ledger: Ledger,
    mode: BrokerMode, limits: &Limits, cancelled: &AtomicBool, progress: &BrokerProgress, input: Option<(File, crate::bounded_worker::BackendIdentity)>)
    -> Result<(BrokerWorkerReport, Option<Vec<u8>>), String> {
    limits.validate()?;
    let pixels = input.is_some();
    let (input, expected_backend) = match input { Some((file, backend)) => (Some(file), Some(backend)), None => (None, None) };
    progress.state.store(0, Ordering::SeqCst);
    let before = ledger.report();
    if before.policy.route != Route::fixture() || before.policy.max_calls != 1
        || before.policy.max_cost_microusd < before.policy.per_call_upper_cost_microusd
        || before.policy.max_output_tokens < 10 || before.halted
        || ((mode != BrokerMode::Inspect || pixels) && before.reserved_calls != 0) {
        return Err(fail("fixture requires a healthy one-call offline ledger with a sufficient known bound"));
    }
    if cancelled.load(Ordering::SeqCst) {
        return Ok((BrokerWorkerReport { worker: Report::cancelled(limits), ledger: before, exchanges: Vec::new(),
            adapter_invocations: 0, channel_error: false, namespace_checks_verified: false,
            synthetic_fixture_only: true, provider_network_used: false, autonomous_execution: false }, None));
    }
    // Only synthetic host secrets/sentinels, never artwork or real credentials.
    // Ordinary exit removes them; SIGKILL can leave this private diagnostic dir.
    let fixture = tempfile::Builder::new().prefix("perveance-broker-probe-")
        .permissions(fs::Permissions::from_mode(0o700)).tempdir().map_err(|_| fail("cannot create private synthetic fixture"))?;
    let private = PrivateDirectory::open(fixture.path())?;
    let mut random = [0u8; 32];
    if unsafe { libc::getrandom(random.as_mut_ptr().cast(), random.len(), 0) } != random.len() as isize {
        return Err(fail("cannot allocate synthetic broker credential"));
    }
    let key = Credential::from_bytes(random.iter().map(|b| format!("{b:02x}")).collect::<String>().into_bytes())?;
    let mut secret = private.create_private("credential.private")?;
    key.with_exposed(|bytes| secret.write_all(bytes.as_bytes())).map_err(|_| fail("cannot write synthetic credential fixture"))?;
    drop(secret);
    let mut sentinel = private.create_private("sentinel.private")?;
    sentinel.write_all(b"synthetic-host-only-sentinel").map_err(|_| fail("cannot write synthetic host sentinel"))?;
    drop(sentinel);
    let socket_path = fixture.path().join("host.sock");
    let _unix = UnixListener::bind(&socket_path).map_err(|_| fail("cannot create synthetic host socket"))?;
    let tcp = TcpListener::bind(("127.0.0.1", 0)).map_err(|_| fail("cannot create synthetic host listener"))?;
    let _tcp_control = std::net::TcpStream::connect(tcp.local_addr().map_err(|_| fail("missing host listener address"))?)
        .map_err(|_| fail("synthetic host TCP control failed"))?;
    // Host-side positive controls: targets exist/really open before worker denial.
    private.read_private("credential.private", 512)?;
    let _control = UnixStream::connect(&socket_path).map_err(|_| fail("synthetic host socket control failed"))?;
    let config = serde_json::json!({"mode": if pixels && mode == BrokerMode::Inspect { "pixel-task" } else { mode.argument() }, "id": before.scope_id,
        "first_status": if before.reserved_calls == 0 { "completed" } else { "already_finished" },
        "private_files": [fixture.path().join("credential.private"), fixture.path().join("sentinel.private"), ledger_root.join(broker::LEDGER_FILE)],
        "host_socket": socket_path, "host_port": tcp.local_addr().map_err(|_| fail("missing host listener address"))?.port(),
        "host_pid": std::process::id()});
    let (mut server, endpoint) = UnixStream::pair().map_err(|_| fail("cannot create broker-worker capability"))?;
    let peer = server.try_clone().map_err(|_| fail("cannot observe broker-worker lease"))?;
    let ready = AtomicBool::new(false);
    let revoked = AtomicBool::new(false);
    let invocations = AtomicU64::new(0);
    let scope = Scope { cancelled, revoked: &revoked, deadline: Instant::now() + Duration::from_secs(limits.wall_seconds), peer: &peer };
    let mut broker = Broker::new(ledger, key, Offline { scope: &scope, mode, progress, invocations: &invocations })?;
    let (worker, exchanges, channel_error, candidate) = thread::scope(|threads| {
        let lease = Revoke(&revoked); // drop on all returns/unwinds BEFORE joining
        let service = threads.spawn(|| {
            while !ready.load(Ordering::SeqCst) && scope.live() { thread::sleep(Duration::from_millis(5)); }
            let mut exchanges: Vec<ServeReport> = Vec::new();
            let mut error = false;
            let count = if mode == BrokerMode::Inspect { EXCHANGES } else { 1 };
            for _ in 0..count {
                if !scope.live() || !ready.load(Ordering::SeqCst) { break; }
                let result = broker.serve_guarded(&mut server, |phase| {
                    if phase == AdmissionPhase::BeforeDispatch {
                        progress.state.store(1, Ordering::SeqCst);
                        if mode == BrokerMode::HoldBeforeDispatch {
                            while scope.live() { thread::sleep(Duration::from_millis(10)); }
                        }
                    }
                    scope.live()
                });
                match result { Ok(report) => exchanges.push(report), Err(_) => { error = true; break; } }
            }
            let candidate = if pixels && mode == BrokerMode::Inspect && !error && exchanges.len() == EXCHANGES {
                match read_candidate(&mut server, &scope) {
                    Ok(bytes) => Some(bytes), Err(_) => { error = true; None }
                }
            } else { None };
            let _ = server.shutdown(Shutdown::Both);
            (exchanges, error, candidate)
        });
        let channel = Channel { ready: &ready, revoked: &revoked, endpoint: Some(endpoint), config: config.to_string(), input, expected_backend };
        let worker = run_inner(snapshot, delegated_root, Mode::Inspect, limits, cancelled, Some(channel));
        drop(lease);
        let (exchanges, error, candidate) = service.join().map_err(|_| fail("trusted offline broker service lost"))?;
        Ok::<_, String>((worker?, exchanges, error, candidate))
    })?;
    let calls = invocations.load(Ordering::SeqCst);
    let verified = worker.outcome == Outcome::Completed;
    if verified {
        let expected = [ReplyStatus::InvalidRequest,
            if before.reserved_calls == 0 { ReplyStatus::Completed } else { ReplyStatus::AlreadyFinished },
            ReplyStatus::AlreadyFinished, ReplyStatus::Conflict, ReplyStatus::BudgetExhausted];
        if channel_error || exchanges.len() != EXCHANGES || exchanges.iter().map(|r| r.status).ne(expected)
            || calls != u64::from(before.reserved_calls == 0) || (pixels && candidate.is_none()) {
            return Err(fail("broker service evidence differs from fixed worker exchange claims"));
        }
    }
    Ok((BrokerWorkerReport { worker, ledger: broker.report(), exchanges, adapter_invocations: calls,
        channel_error, namespace_checks_verified: verified, synthetic_fixture_only: true,
        provider_network_used: false, autonomous_execution: false }, candidate))
}

fn read_candidate(peer: &mut UnixStream, scope: &Scope<'_>) -> Result<Vec<u8>, String> {
    // Data after peer exit is permitted only here, not in admission: a full
    // bounded frame + EOF is verified after normal completion/whole-tree cleanup.
    fn read(peer: &mut UnixStream, out: &mut [u8], scope: &Scope<'_>) -> Result<(), String> {
        let mut offset = 0;
        while offset < out.len() {
            if scope.cancelled.load(Ordering::SeqCst) { return Err(fail("candidate transport cancelled")); }
            let remaining = scope.deadline.checked_duration_since(Instant::now()).ok_or_else(|| fail("candidate deadline expired"))?;
            let mut fd = libc::pollfd { fd: peer.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            if unsafe { libc::poll(&mut fd, 1, remaining.as_millis().saturating_add(1).min(100) as i32) } < 0 {
                return Err(fail("candidate channel poll failed"));
            }
            match peer.read(&mut out[offset..]) {
                Ok(0) => return Err(fail("incomplete candidate frame")), Ok(n) => offset += n,
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted) => {},
                Err(_) => return Err(fail("candidate channel unavailable")),
            }
        }
        Ok(())
    }
    let mut size = [0u8; 4]; read(peer, &mut size, scope)?;
    let size = u32::from_be_bytes(size) as usize;
    if size == 0 || size as u64 > crate::MAX_PROJECT_BYTES { return Err(fail("candidate size rejected before allocation")); }
    let mut bytes = vec![0; size]; read(peer, &mut bytes, scope)?;
    loop {
        if scope.cancelled.load(Ordering::SeqCst) || Instant::now() >= scope.deadline { return Err(fail("candidate closing deadline expired")); }
        let mut tail = [0u8; 1];
        match peer.read(&mut tail) {
            Ok(0) => return Ok(bytes), Ok(_) => return Err(fail("trailing candidate bytes rejected")),
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted) => thread::sleep(Duration::from_millis(5)),
            Err(_) => return Err(fail("candidate closing failed")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn buffered_request_does_not_keep_a_closed_or_half_closed_peer_authorized() {
        for half in [true, false] {
            let (server, mut client) = UnixStream::pair().unwrap();
            assert!(peer_live(&server));
            client.write_all(b"queued request").unwrap();
            assert!(peer_live(&server));
            if half { client.shutdown(Shutdown::Write).unwrap(); } else { drop(client); }
            assert!(!peer_live(&server));
        }
    }
    #[test]
    fn candidate_frame_requires_one_bounded_complete_frame_and_eof() {
        for case in 0..5 {
            let (mut server, mut client) = UnixStream::pair().unwrap();
            server.set_nonblocking(true).unwrap();
            let observer = server.try_clone().unwrap();
            let cancelled = AtomicBool::new(false); let revoked = AtomicBool::new(false);
            let scope = Scope { cancelled: &cancelled, revoked: &revoked, peer: &observer,
                deadline: Instant::now() + Duration::from_millis(150) };
            let bytes = match case {
                0 => [3u32.to_be_bytes().as_slice(), b"abc"].concat(),
                1 => ((crate::MAX_PROJECT_BYTES + 1) as u32).to_be_bytes().to_vec(),
                2 => [3u32.to_be_bytes().as_slice(), b"ab"].concat(),
                3 => [3u32.to_be_bytes().as_slice(), b"abcX"].concat(),
                _ => 0u32.to_be_bytes().to_vec(),
            };
            for byte in bytes { client.write_all(&[byte]).unwrap(); }
            drop(client);
            let result = read_candidate(&mut server, &scope);
            if case == 0 { assert_eq!(result.unwrap(), b"abc"); } else { assert!(result.is_err()); }
        }
    }

    #[test]
    fn cancellation_revocation_and_expired_deadlines_cannot_be_renewed_by_data() {
        let (server, mut client) = UnixStream::pair().unwrap();
        let cancelled = AtomicBool::new(false); let revoked = AtomicBool::new(false);
        let mut scope = Scope { cancelled: &cancelled, revoked: &revoked, peer: &server, deadline: Instant::now()+Duration::from_secs(1) };
        assert!(scope.live()); client.write_all(b"buffered").unwrap();
        cancelled.store(true, Ordering::SeqCst); assert!(!scope.live());
        cancelled.store(false, Ordering::SeqCst); revoked.store(true, Ordering::SeqCst); assert!(!scope.live());
        revoked.store(false, Ordering::SeqCst); scope.deadline=Instant::now(); assert!(!scope.live());
    }
}

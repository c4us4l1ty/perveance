#[cfg(target_os = "linux")]
mod linux {
    use perveance::{broker::{self, Broker, CallRequest, CallState, Credential, FixtureTransport, Ledger, Policy,
        ProviderCall, Receipt, ReplyStatus, Route, Transport, TransportFailure, LEDGER_FILE}, worker_io::PrivateDirectory};
    use std::{fs, os::{fd::{AsRawFd, OwnedFd}, unix::{fs::{symlink, PermissionsExt},
        net::UnixStream, process::{CommandExt, ExitStatusExt}}}, path::Path,
        process::{Child, Command, Stdio}, thread, time::{Duration, Instant}};

    const KEY: &[u8] = b"synthetic-test-broker-secret-marker";
    fn private() -> tempfile::TempDir {
        tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o700)).tempdir().unwrap()
    }
    fn root(path: &Path) -> PrivateDirectory { PrivateDirectory::open(path).unwrap() }
    fn request(n: u64) -> CallRequest {
        CallRequest { version: 1, id: format!("{n:032x}"), prompt: "private-integration-prompt-marker".into(), max_output_tokens: 10 }
    }
    fn cli(args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_perveance-cli")).args(args).output().unwrap()
    }
    fn text(path: &Path) -> &str { path.to_str().unwrap() }
    fn write_private(path: &Path, bytes: &[u8]) {
        fs::write(path, bytes).unwrap(); fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    fn write_request(path: &Path, n: u64) { write_private(path, &serde_json::to_vec(&request(n)).unwrap()); }
    fn fifo(path: &Path) {
        let name = std::ffi::CString::new(text(path)).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    }

    #[test]
    fn cli_round_trip_is_offline_durable_no_refund_and_deduplicated() {
        let ledger = private(); let input = private();
        let init = cli(&["broker-ledger-init", text(ledger.path()), "2", "20", "10"]);
        assert!(init.status.success(), "{}", String::from_utf8_lossy(&init.stderr));
        write_request(&input.path().join("call.json"), 1);
        let args = ["broker-fixture-call", text(ledger.path()), text(input.path()), "call.json"];
        let mut states = Vec::new();
        for _ in 0..2 {
            let result = cli(&args);
            assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
            let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
            assert_eq!(report["fixture"]["fixture_only"], true);
            assert_eq!(report["fixture"]["network_used"], false);
            assert_eq!(report["fixture"]["autonomous_execution"], false);
            assert_eq!(report["fixture"]["ledger"]["reserved_calls"], 1);
            assert_eq!(report["fixture"]["ledger"]["reserved_cost_microusd"], 10);
            states.push(report["fixture"]["reply"]["status"].as_str().unwrap().to_owned());
            assert!(!String::from_utf8_lossy(&result.stdout).contains("private-integration-prompt-marker"));
            assert!(!String::from_utf8_lossy(&result.stdout).contains("synthetic-offline-broker-credential"));
        }
        assert_eq!(states, ["completed", "already_finished"]);
        for (n, expected) in [(2, "completed"), (3, "budget_exhausted")] {
            write_request(&input.path().join("call.json"), n);
            let result = cli(&args); assert!(result.status.success());
            let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
            assert_eq!(report["fixture"]["reply"]["status"], expected);
            assert_eq!(report["fixture"]["ledger"]["reserved_cost_microusd"], 20);
        }
        let before = fs::read(ledger.path().join(LEDGER_FILE)).unwrap();
        assert!(!cli(&["broker-ledger-init", text(ledger.path()), "128", "10000", "1"]).status.success());
        assert_eq!(fs::read(ledger.path().join(LEDGER_FILE)).unwrap(), before);
        let inspect = cli(&["broker-ledger-inspect", text(ledger.path())]);
        assert!(inspect.status.success());
        let report: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
        assert_eq!(report["ledger"]["reserved_calls"], 2);
        assert_eq!(report["ledger"]["halted"], false);
        assert_eq!(fs::metadata(ledger.path().join(LEDGER_FILE)).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::read_dir(ledger.path()).unwrap().count(), 1);
    }

    #[test]
    fn cli_discovery_limits_errors_and_intake_do_not_enable_a_provider() {
        let result = cli(&["capabilities"]);
        let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(value["model_broker"]["durable_reservations"], true);
        assert_eq!(value["model_broker"]["task_worker_provider_integration"], false);
        assert_eq!(value["model_broker"]["autonomous_execution"], false);
        assert_eq!(value["autonomy_intake"]["budgets_enforced"], false);
        assert_eq!(value["autonomous_execution"], false);
        let directory = private();
        for args in [
            vec!["broker-ledger-init", text(directory.path()), "129", "10", "1"],
            vec!["broker-ledger-init", text(directory.path()), "1", "10", "0"],
            vec!["broker-ledger-init", text(directory.path()), "private-budget-marker", "10", "1"],
        ] {
            let result = cli(&args); assert!(!result.status.success());
            assert!(!String::from_utf8_lossy(&result.stderr).contains("private-budget-marker"));
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn occupied_ledger_objects_and_links_are_not_activated_replaced_or_reset() {
        for kind in 0..5 {
            let directory = private(); let outside = private();
            let path = directory.path().join(LEDGER_FILE);
            let sentinel = outside.path().join("sentinel"); write_private(&sentinel, b"private-sentinel");
            match kind {
                0 => write_private(&path, b"occupied-ledger"),
                1 => fs::create_dir(&path).unwrap(),
                2 => symlink(&sentinel, &path).unwrap(),
                3 => fifo(&path),
                _ => fs::hard_link(&sentinel, &path).unwrap(),
            }
            assert!(!cli(&["broker-ledger-init", text(directory.path()), "1", "10", "10"]).status.success());
            assert!(!cli(&["broker-ledger-inspect", text(directory.path())]).status.success());
            assert_eq!(fs::read(&sentinel).unwrap(), b"private-sentinel");
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
            assert!(fs::symlink_metadata(path).is_ok());
        }
    }

    #[test]
    fn request_capture_and_credential_capture_reject_unsafe_sources_without_spending() {
        let directory = private(); let inputs = private(); let secrets = private();
        drop(Ledger::create(root(directory.path()), Policy::fixture(2, 20, 10)).unwrap());
        let secret = secrets.path().join("credential"); write_private(&secret, KEY);
        let credential = Credential::from_private_file(&root(secrets.path()), "credential").unwrap();
        assert_eq!(format!("{credential:?}"), "Credential(REDACTED)");
        credential.with_exposed(|key| assert_eq!(key.as_bytes(), KEY));
        symlink(&secret, inputs.path().join("link")).unwrap();
        fifo(&inputs.path().join("fifo"));
        write_private(&inputs.path().join("bad.json"), b"private-malformed-marker");
        let large = inputs.path().join("large.json"); write_private(&large, b"x");
        fs::OpenOptions::new().write(true).open(&large).unwrap().set_len(broker::MAX_REQUEST_FRAME as u64 + 1).unwrap();
        for name in ["../credential", "link", "fifo", "bad.json", "large.json"] {
            let result = cli(&["broker-fixture-call", text(directory.path()), text(inputs.path()), name]);
            assert!(!result.status.success());
            assert!(!String::from_utf8_lossy(&result.stderr).contains("private-malformed-marker"));
        }
        assert!(Credential::from_private_file(&root(inputs.path()), "link").is_err());
        assert!(Credential::from_private_file(&root(inputs.path()), "fifo").is_err());
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(Credential::from_private_file(&root(secrets.path()), "credential").is_err());
        assert_eq!(Ledger::open(root(directory.path())).unwrap().report().reserved_calls, 0);
    }

    #[test]
    fn cooperating_processes_cannot_oversubscribe_or_bypass_a_live_lock() {
        let directory = private(); let inputs = private();
        let ledger = Ledger::create(root(directory.path()), Policy::fixture(1, 10, 10)).unwrap();
        write_request(&inputs.path().join("call-0.json"), 0);
        assert!(!cli(&["broker-ledger-inspect", text(directory.path())]).status.success());
        assert!(!cli(&["broker-fixture-call", text(directory.path()), text(inputs.path()), "call-0.json"]).status.success());
        drop(ledger);
        let mut children = Vec::new();
        for n in 1..=8 {
            let name = format!("call-{n}.json"); write_request(&inputs.path().join(&name), n);
            children.push(Command::new(env!("CARGO_BIN_EXE_perveance-cli"))
                .args(["broker-fixture-call", text(directory.path()), text(inputs.path()), &name])
                .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap());
        }
        let mut completed = 0;
        for child in children {
            let output = child.wait_with_output().unwrap();
            if output.status.success() {
                let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                if value["fixture"]["reply"]["status"] == "completed" { completed += 1; }
                else { assert_eq!(value["fixture"]["reply"]["status"], "budget_exhausted"); }
            }
        }
        assert_eq!(completed, 1);
        let report = Ledger::open(root(directory.path())).unwrap().report();
        assert_eq!((report.reserved_calls, report.reserved_cost_microusd), (1, 10));
    }

    #[test]
    fn separate_fixed_peer_gets_only_a_protocol_stream_not_credential_fds_or_environment() {
        let directory = private(); let secrets = private();
        let secret_path = secrets.path().join("private-key"); write_private(&secret_path, KEY);
        let credential = Credential::from_private_file(&root(secrets.path()), "private-key").unwrap();
        // Deliberately make a synthetic host file inheritable, then require the
        // test launcher hygiene to remove it. This is descriptor/protocol proof,
        // NOT kernel namespace or hostile same-user isolation evidence.
        let secret_file = fs::File::open(&secret_path).unwrap();
        assert_eq!(unsafe { libc::fcntl(secret_file.as_raw_fd(), libc::F_SETFD, 0) }, 0);
        let ledger = Ledger::create(root(directory.path()), Policy::fixture(1, 10, 10)).unwrap();
        let mut broker = Broker::new(ledger, credential, FixtureTransport).unwrap();
        let (server, client) = UnixStream::pair().unwrap();
        let script = r#"
import json, os, socket, struct
s = socket.socket(fileno=0)
s.settimeout(3)
def exact(n):
    out = b''
    while len(out) < n:
        part = s.recv(n-len(out))
        if not part: raise RuntimeError('closed protocol fixture')
        out += part
    return out
extra = []
for name in os.listdir('/proc/self/fd'):
    if int(name) > 2:
        try: extra.append(os.readlink('/proc/self/fd/' + name))
        except FileNotFoundError: pass
assert extra == []
assert 'OPENROUTER_API_KEY' not in os.environ
req = {'version':1, 'id':'00000000000000000000000000000001', 'prompt':'fixed peer request', 'max_output_tokens':10}
b = json.dumps(req).encode()
s.sendall(struct.pack('!I',len(b)) + b)
size = struct.unpack('!I', exact(4))[0]
assert 0 < size < 65536
reply = json.loads(exact(size))
assert reply['status'] == 'completed'
print(json.dumps({'reply':reply, 'extra_fds':extra}))
"#;
        let mut command = Command::new("/usr/bin/python3");
        command.args(["-I", "-c", script]).env_clear().stdin(Stdio::from(OwnedFd::from(client)))
            .stdout(Stdio::piped()).stderr(Stdio::piped());
        unsafe { command.pre_exec(|| {
            if libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 4u32) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        }); }
        let child = command.spawn().unwrap();
        let served = broker.serve_once(server);
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(served.unwrap().status, ReplyStatus::Completed);
        assert!(!String::from_utf8_lossy(&output.stdout).contains(std::str::from_utf8(KEY).unwrap()));
        assert_eq!(broker.report().reserved_calls, 1);
    }

    struct ParkTransport { marker: std::path::PathBuf }
    impl Transport for ParkTransport {
        fn route(&self) -> Route { Route::fixture() }
        fn complete(&mut self, _credential: &Credential, _call: ProviderCall<'_>) -> Result<Receipt, TransportFailure> {
            fs::write(&self.marker, b"entered adapter after durable reservation").unwrap();
            loop { thread::park(); }
        }
    }

    #[test]
    fn crash_fixture_helper() {
        let Some(path) = std::env::var_os("PERVEANCE_BROKER_CRASH_FIXTURE") else { return; };
        let path = std::path::PathBuf::from(path);
        let ledger = Ledger::open(root(&path)).unwrap();
        let mut broker = Broker::new(ledger, Credential::from_bytes(KEY.to_vec()).unwrap(),
            ParkTransport { marker: path.join("dispatch-marker") }).unwrap();
        let (server, client) = UnixStream::pair().unwrap();
        thread::spawn(move || { let _ = broker::client_call(client, &request(1)); });
        let _ = broker.serve_once(server);
        panic!("blocking fixture unexpectedly returned");
    }

    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
    }

    #[test]
    fn sigkill_after_dispatch_keeps_full_reservation_and_cannot_retry_or_refund() {
        let directory = private();
        drop(Ledger::create(root(directory.path()), Policy::fixture(3, 30, 10)).unwrap());
        let mut child = ChildGuard(Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "linux::crash_fixture_helper", "--nocapture", "--test-threads=1"])
            .env("PERVEANCE_BROKER_CRASH_FIXTURE", directory.path())
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::inherit()).spawn().unwrap());
        let start = Instant::now();
        while !directory.path().join("dispatch-marker").exists() {
            assert!(child.0.try_wait().unwrap().is_none(), "crash fixture exited before dispatch");
            assert!(start.elapsed() < Duration::from_secs(10), "crash fixture never dispatched");
            thread::sleep(Duration::from_millis(10));
        }
        child.0.kill().unwrap();
        assert_eq!(child.0.wait().unwrap().signal(), Some(libc::SIGKILL));
        let ledger = Ledger::open(root(directory.path())).unwrap();
        let report = ledger.report();
        assert_eq!((report.reserved_calls, report.reserved_cost_microusd), (1, 10));
        assert_eq!(report.calls[0].state, CallState::ReservedUncertain);
        assert!(report.halted);
        let mut broker = Broker::new(ledger, Credential::from_bytes(KEY.to_vec()).unwrap(), FixtureTransport).unwrap();
        for (n, status) in [(1, ReplyStatus::OutcomeUnknown), (2, ReplyStatus::Halted)] {
            let (server, client) = UnixStream::pair().unwrap();
            thread::scope(|scope| {
                let client = scope.spawn(move || broker::client_call(client, &request(n)).unwrap());
                let served = broker.serve_once(server).unwrap();
                assert!(!served.provider_called);
                assert_eq!(client.join().unwrap().status, status);
            });
        }
        assert_eq!(broker.report().reserved_cost_microusd, 10);
    }
}

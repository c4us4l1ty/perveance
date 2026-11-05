use super::*;
use std::{fs, os::unix::fs::PermissionsExt, sync::{Arc, atomic::{AtomicUsize, Ordering}}};

const KEY: &[u8] = b"synthetic-private-credential-marker";
fn private() -> tempfile::TempDir {
    tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o700)).tempdir().unwrap()
}
fn root(path: &std::path::Path) -> PrivateDirectory { PrivateDirectory::open(path).unwrap() }
fn policy() -> Policy { Policy::fixture(3, 30, 10) }
fn request(n: u64) -> CallRequest {
    CallRequest { version: VERSION, id: format!("{n:032x}"), prompt: "private-prompt-marker".into(), max_output_tokens: 50 }
}
fn credential() -> Credential { Credential::from_bytes(KEY.to_vec()).unwrap() }
fn broker(path: &std::path::Path) -> Broker<FixtureTransport> {
    Broker::new(Ledger::open(root(path)).unwrap(), credential(), FixtureTransport).unwrap()
}
fn exchange<T: Transport>(broker: &mut Broker<T>, request: &CallRequest) -> Reply {
    let (server, client) = UnixStream::pair().unwrap();
    std::thread::scope(|scope| {
        let peer = scope.spawn(|| client_call(client, request).unwrap());
        broker.serve_once(server).unwrap();
        peer.join().unwrap()
    })
}

#[test]
fn policy_request_and_credential_validation_are_bounded_and_redacted() {
    for bad in [0, 4097] {
        let mut value = policy(); value.max_output_tokens = bad;
        assert!(value.validate().is_err());
    }
    let mut value = policy(); value.max_calls = MAX_CALLS + 1;
    assert!(value.validate().is_err());
    value = policy(); value.per_call_upper_cost_microusd = 0;
    assert!(value.validate().is_err());
    for bytes in [Vec::new(), b"short-secret".to_vec(), vec![b'a'; 513], vec![b'\n'; 32]] {
        let error = Credential::from_bytes(bytes).unwrap_err();
        assert!(!error.contains("short-secret"));
    }
    assert_eq!(format!("{:?}", credential()), "Credential(REDACTED)");
    assert!(!format!("{:?}", request(1)).contains("private-prompt-marker"));
    for field in 0..5 {
        let mut r = request(1);
        match field {
            0 => r.version = 2, 1 => r.id = "../not-a-call".into(),
            2 => r.prompt = " ".into(), 3 => r.prompt = "é".repeat(2001), _ => r.max_output_tokens = 1025,
        }
        assert!(r.validate(&policy()).is_err());
    }
    let mut r = request(1); r.prompt = "é".repeat(2000);
    r.validate(&policy()).unwrap();
}

#[test]
fn reservations_survive_restarts_never_refund_and_reject_changed_replays() {
    let directory = private();
    drop(Ledger::create(root(directory.path()), policy()).unwrap());
    {
        let mut b = broker(directory.path());
        let reply = exchange(&mut b, &request(1));
        assert_eq!(reply.status, ReplyStatus::Completed);
        assert_eq!(b.report().reserved_cost_microusd, 10);
        assert_eq!(b.report().calls[0].actual_cost_microusd, Some(5));
        assert!(!b.report().halted);
    }
    let mut b = broker(directory.path());
    assert_eq!(exchange(&mut b, &request(1)).status, ReplyStatus::AlreadyFinished);
    let mut changed = request(1); changed.prompt.push('!');
    assert_eq!(exchange(&mut b, &changed).status, ReplyStatus::Conflict);
    changed = request(1); changed.max_output_tokens += 1;
    assert_eq!(exchange(&mut b, &changed).status, ReplyStatus::Conflict);
    assert_eq!(exchange(&mut b, &request(2)).status, ReplyStatus::Completed);
    assert_eq!(exchange(&mut b, &request(3)).status, ReplyStatus::Completed);
    assert_eq!(exchange(&mut b, &request(4)).status, ReplyStatus::BudgetExhausted);
    assert_eq!((b.report().reserved_calls, b.report().reserved_cost_microusd), (3, 30));
    assert!(exchange(&mut b, &request(1)).text.is_none());
    let disk = fs::read(directory.path().join(LEDGER_FILE)).unwrap();
    for marker in [KEY, b"private-prompt-marker", b"Fixed offline broker response"] {
        assert!(!disk.windows(marker.len()).any(|part| part == marker));
    }
    let metadata = serde_json::to_string(&b.report()).unwrap();
    assert!(!metadata.contains("fingerprint") && !metadata.contains("private-prompt-marker"));
}

#[test]
fn zero_ceilings_cost_exhaustion_and_overflow_cannot_dispatch() {
    for p in [Policy::fixture(0, 100, 10), Policy::fixture(5, 0, 10), Policy::fixture(5, 9, 10)] {
        let directory = private();
        drop(Ledger::create(root(directory.path()), p).unwrap());
        let mut b = broker(directory.path());
        assert_eq!(exchange(&mut b, &request(1)).status, ReplyStatus::BudgetExhausted);
        assert_eq!(b.report().reserved_calls, 0);
    }
    let directory = private();
    drop(Ledger::create(root(directory.path()), Policy::fixture(2, u64::MAX, u64::MAX)).unwrap());
    let mut b = broker(directory.path());
    assert_eq!(exchange(&mut b, &request(1)).status, ReplyStatus::Completed);
    assert_eq!(exchange(&mut b, &request(2)).status, ReplyStatus::BudgetExhausted);
    assert_eq!(b.report().reserved_cost_microusd, u64::MAX);
}

#[test]
fn exclusive_lock_is_not_a_shared_offset_clone_and_root_replacement_does_not_redirect() {
    let parent = private();
    let original = parent.path().join("original");
    fs::create_dir(&original).unwrap(); fs::set_permissions(&original, fs::Permissions::from_mode(0o700)).unwrap();
    let ledger = Ledger::create(root(&original), policy()).unwrap();
    assert!(Ledger::open(root(&original)).is_err());
    let moved = parent.path().join("moved");
    fs::rename(&original, &moved).unwrap();
    fs::create_dir(&original).unwrap(); fs::set_permissions(&original, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(original.join(LEDGER_FILE), b"replacement-sentinel").unwrap();
    let mut b = Broker::new(ledger, credential(), FixtureTransport).unwrap();
    assert_eq!(exchange(&mut b, &request(1)).status, ReplyStatus::Completed);
    assert_eq!(fs::read(original.join(LEDGER_FILE)).unwrap(), b"replacement-sentinel");
    drop(b);
    assert_eq!(Ledger::open(root(&moved)).unwrap().report().reserved_calls, 1);
}

#[test]
fn all_sync_fault_points_disable_dispatch_and_preserve_visible_reservations() {
    for phase in [Phase::Written, Phase::FileSynced, Phase::DirectorySynced] {
        let directory = private();
        let mut ledger = Ledger::create(root(directory.path()), policy()).unwrap();
        let r = request(1);
        let record = Record::Reserved { sequence: ledger.records, id: r.id.clone(),
            fingerprint: ledger.fingerprint(&r).unwrap(), cost_microusd: 10 };
        assert!(ledger.append_with_hook(&record, |at| if at == phase { Err(()) } else { Ok(()) }).is_err());
        assert!(ledger.report().halted && ledger.report().accounting_uncertain);
        assert_eq!(ledger.reserve(&request(2)).unwrap(), Some(ReplyStatus::Halted));
        drop(ledger);
        let reopened = Ledger::open(root(directory.path())).unwrap();
        assert_eq!(reopened.report().reserved_calls, 1);
        assert_eq!(reopened.report().calls[0].state, CallState::ReservedUncertain);
        assert!(reopened.report().halted);
        let mut b = Broker::new(reopened, credential(), FixtureTransport).unwrap();
        assert_eq!(exchange(&mut b, &r).status, ReplyStatus::OutcomeUnknown);
        assert_eq!(exchange(&mut b, &request(2)).status, ReplyStatus::Halted);
    }
}

#[test]
fn uncertain_settlement_sync_never_allows_a_second_dispatch_or_refund() {
    for phase in [Phase::Written, Phase::FileSynced, Phase::DirectorySynced] {
        let directory = private();
        let mut ledger = Ledger::create(root(directory.path()), policy()).unwrap();
        assert!(ledger.reserve(&request(1)).unwrap().is_none());
        let record = Record::Finished { sequence: ledger.records, id: request(1).id,
            state: CallState::Completed, actual_cost_microusd: Some(1), response_bytes: 20 };
        assert!(ledger.append_with_hook(&record, |at| if at == phase { Err(()) } else { Ok(()) }).is_err());
        assert!(ledger.report().halted && ledger.report().accounting_uncertain);
        drop(ledger);
        let mut b = broker(directory.path());
        assert_eq!(exchange(&mut b, &request(1)).status, ReplyStatus::AlreadyFinished);
        assert_eq!(b.report().reserved_cost_microusd, 10);
    }
}

struct Adverse { kind: u8, count: Arc<AtomicUsize> }
impl Transport for Adverse {
    fn route(&self) -> Route { Route::fixture() }
    fn complete(&mut self, credential: &Credential, call: ProviderCall<'_>) -> Result<Receipt, TransportFailure> {
        self.count.fetch_add(1, Ordering::SeqCst);
        match self.kind {
            0 => Err(TransportFailure::OutcomeUnknown),
            1 => Ok(Receipt { text: "unknown billing".into(), actual_cost_microusd: None }),
            2 => Ok(Receipt { text: "over bound".into(), actual_cost_microusd: Some(call.upper_cost_microusd + 1) }),
            3 => Ok(Receipt { text: credential.with_exposed(str::to_owned), actual_cost_microusd: Some(1) }),
            4 => Ok(Receipt { text: "x".repeat(MAX_COMPLETION_BYTES + 1), actual_cost_microusd: Some(1) }),
            _ => Ok(Receipt { text: String::new(), actual_cost_microusd: Some(1) }),
        }
    }
}

#[test]
fn unknown_billing_breaches_invalid_text_and_key_echo_latch_closed_without_leaking() {
    for kind in 0..6 {
        let directory = private();
        let ledger = Ledger::create(root(directory.path()), policy()).unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let mut b = Broker::new(ledger, credential(), Adverse { kind, count: count.clone() }).unwrap();
        let reply = exchange(&mut b, &request(1));
        assert_eq!(reply.status, match kind {
            0 | 1 => ReplyStatus::OutcomeUnknown, 2 => ReplyStatus::BillingBoundViolated,
            _ => ReplyStatus::ProviderRejected,
        });
        assert!(reply.text.is_none());
        assert!(!serde_json::to_string(&reply).unwrap().contains(std::str::from_utf8(KEY).unwrap()));
        assert!(b.report().halted);
        assert_eq!(exchange(&mut b, &request(2)).status, ReplyStatus::Halted);
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(b.report().reserved_cost_microusd, 10);
        drop(b);
        assert!(Ledger::open(root(directory.path())).unwrap().report().halted);
    }
}

#[test]
fn malformed_frames_and_worker_overrides_do_not_spend_or_echo_input() {
    let directory = private();
    drop(Ledger::create(root(directory.path()), policy()).unwrap());
    let mut b = broker(directory.path());
    let base = serde_json::to_value(request(1)).unwrap();
    let mut inputs = vec![b"private-malformed-marker".to_vec(), b"{\"version\":1,\"version\":1}".to_vec()];
    for field in ["key", "model", "url", "upper_cost_microusd", "max_calls", "route", "refund"] {
        let mut value = base.clone(); value[field] = "private-override-marker".into();
        inputs.push(serde_json::to_vec(&value).unwrap());
    }
    for bytes in inputs {
        let (server, mut client) = UnixStream::pair().unwrap();
        std::thread::scope(|scope| {
            let peer = scope.spawn(|| {
                client.set_nonblocking(true).unwrap();
                write_frame(&mut client, &bytes, MAX_REQUEST_FRAME, Instant::now() + IO_DEADLINE).unwrap();
                let response = read_frame(&mut client, MAX_REPLY_FRAME, Instant::now() + IO_DEADLINE).unwrap();
                assert!(!String::from_utf8_lossy(&response).contains("private-"));
                let reply: Reply = serde_json::from_slice(&response).unwrap();
                assert_eq!(reply.status, ReplyStatus::InvalidRequest);
            });
            assert!(!b.serve_once(server).unwrap().provider_called);
            peer.join().unwrap();
        });
    }
    for length in [0u32, MAX_REQUEST_FRAME as u32 + 1, u32::MAX] {
        let (server, mut client) = UnixStream::pair().unwrap();
        std::thread::scope(|scope| {
            let peer = scope.spawn(|| {
                client.write_all(&length.to_be_bytes()).unwrap();
                let bytes = read_frame(&mut client, MAX_REPLY_FRAME, Instant::now() + IO_DEADLINE).unwrap();
                assert_eq!(serde_json::from_slice::<Reply>(&bytes).unwrap().status, ReplyStatus::InvalidRequest);
            });
            assert!(!b.serve_once(server).unwrap().provider_called);
            peer.join().unwrap();
        });
    }
    let mut leaked = request(1); leaked.prompt = std::str::from_utf8(KEY).unwrap().into();
    assert_eq!(exchange(&mut b, &leaked).status, ReplyStatus::InvalidRequest);
    assert_eq!(b.report().reserved_calls, 0);
}

#[test]
fn fragmented_and_dripping_frames_use_one_absolute_deadline() {
    let directory = private();
    drop(Ledger::create(root(directory.path()), policy()).unwrap());
    let mut b = broker(directory.path());
    let (server, mut client) = UnixStream::pair().unwrap();
    let r = request(1);
    let bytes = serde_json::to_vec(&r).unwrap();
    std::thread::scope(|scope| {
        let peer = scope.spawn(|| {
            for byte in (bytes.len() as u32).to_be_bytes().into_iter().chain(bytes.iter().copied()) {
                client.write_all(&[byte]).unwrap();
            }
            let bytes = read_frame(&mut client, MAX_REPLY_FRAME, Instant::now() + IO_DEADLINE).unwrap();
            assert_eq!(serde_json::from_slice::<Reply>(&bytes).unwrap().status, ReplyStatus::Completed);
        });
        b.serve_once(server).unwrap(); peer.join().unwrap();
    });
    // Use a short direct protocol deadline rather than a wall-clock-heavy
    // production timeout. Each byte arrives, but the whole frame cannot finish.
    let (mut server, mut client) = UnixStream::pair().unwrap();
    server.set_nonblocking(true).unwrap();
    let start = Instant::now();
    std::thread::scope(|scope| {
        let peer = scope.spawn(|| {
            for byte in 100u32.to_be_bytes().into_iter().chain([0u8; 100]) {
                if client.write_all(&[byte]).is_err() { break; }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        assert!(read_frame(&mut server, MAX_REQUEST_FRAME, start + Duration::from_millis(45)).is_err());
        drop(server); // stop the writer; a reset-per-byte reader would accept the whole frame
        peer.join().unwrap();
    });
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn lost_reply_after_dispatch_retains_accounting_and_is_not_replayed() {
    let directory = private();
    drop(Ledger::create(root(directory.path()), policy()).unwrap());
    let mut b = broker(directory.path());
    let (server, mut client) = UnixStream::pair().unwrap();
    let bytes = serde_json::to_vec(&request(1)).unwrap();
    client.write_all(&(bytes.len() as u32).to_be_bytes()).unwrap();
    client.write_all(&bytes).unwrap();
    client.shutdown(std::net::Shutdown::Read).unwrap();
    // The request was delivered, but the client cannot receive the response.
    let _ = b.serve_once(server);
    assert_eq!(b.report().reserved_calls, 1);
    drop(client); drop(b);
    let mut b = broker(directory.path());
    let reply = exchange(&mut b, &request(1));
    assert_eq!(reply.status, ReplyStatus::AlreadyFinished);
    assert!(reply.text.is_none());
}

#[test]
fn partial_tails_checksums_reordering_and_invalid_transitions_fail_closed_without_repair() {
    let directory = private();
    drop(Ledger::create(root(directory.path()), policy()).unwrap());
    {
        let mut b = broker(directory.path());
        exchange(&mut b, &request(1));
    }
    let path = directory.path().join(LEDGER_FILE);
    let complete = fs::read(&path).unwrap();
    let mut corruptions = Vec::new();
    corruptions.push(complete[..complete.len() - 1].to_vec());
    let mut flip = complete.clone(); let last = flip.len() - 1; flip[last] ^= 1; corruptions.push(flip);
    let mut tail = complete.clone(); tail.extend_from_slice(&[0, 0]); corruptions.push(tail);
    let header_end = 12 + u32::from_be_bytes(complete[8..12].try_into().unwrap()) as usize + 32;
    let reserved_end = header_end + 4 + u32::from_be_bytes(complete[header_end..header_end+4].try_into().unwrap()) as usize + 32;
    let mut reordered = complete[..header_end].to_vec();
    reordered.extend_from_slice(&complete[reserved_end..]); reordered.extend_from_slice(&complete[header_end..reserved_end]);
    corruptions.push(reordered);
    let mut length = complete.clone(); length[8..12].copy_from_slice(&u32::MAX.to_be_bytes()); corruptions.push(length);
    // Recompute checksums around semantically invalid records: checksums alone
    // are not authorization or a substitute for the state machine.
    for forged in [
        Record::Reserved { sequence: 1, id: request(1).id, fingerprint: "0".repeat(64), cost_microusd: 1 },
        Record::Finished { sequence: 1, id: request(1).id, state: CallState::Completed,
            actual_cost_microusd: Some(0), response_bytes: 1 },
        Record::Header { version: VERSION, scope_id: "0".repeat(32), policy: policy() },
    ] {
        let header_len = u32::from_be_bytes(complete[8..12].try_into().unwrap()) as usize;
        let end = 12 + header_len + 32;
        let mut forged_bytes = complete[..end].to_vec();
        let previous: [u8; 32] = complete[end-32..end].try_into().unwrap();
        let payload = serde_json::to_vec(&forged).unwrap();
        forged_bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        forged_bytes.extend_from_slice(&payload); forged_bytes.extend_from_slice(&digest(&previous, &payload));
        corruptions.push(forged_bytes);
    }
    for corrupt in corruptions {
        fs::write(&path, &corrupt).unwrap();
        assert!(Ledger::open(root(directory.path())).is_err());
        assert_eq!(fs::read(&path).unwrap(), corrupt);
        assert!(Ledger::create(root(directory.path()), policy()).is_err());
        assert_eq!(fs::read(&path).unwrap(), corrupt);
    }
}

#[test]
fn route_mismatch_never_constructs_an_authorized_broker() {
    let directory = private();
    let mut p = policy(); p.route.model = "different-model".into();
    let ledger = Ledger::create(root(directory.path()), p).unwrap();
    assert!(Broker::new(ledger, credential(), FixtureTransport).is_err());
    assert_eq!(Ledger::open(root(directory.path())).unwrap().report().reserved_calls, 0);
}


#[test]
fn reservation_sync_failure_through_real_broker_path_never_invokes_transport() {
    for phase in [Phase::Written, Phase::FileSynced, Phase::DirectorySynced] {
        let directory = private();
        let mut ledger = Ledger::create(root(directory.path()), policy()).unwrap();
        ledger.test_fault = Some((1, phase));
        let count = Arc::new(AtomicUsize::new(0));
        let mut b = Broker::new(ledger, credential(), Adverse { kind: 0, count: count.clone() }).unwrap();
        let reply = exchange(&mut b, &request(1));
        assert_eq!(reply.status, ReplyStatus::AccountingUncertain);
        assert!(reply.text.is_none() && b.report().halted && b.report().accounting_uncertain);
        assert_eq!(count.load(Ordering::SeqCst), 0);
        drop(b);
        assert_eq!(Ledger::open(root(directory.path())).unwrap().report().reserved_cost_microusd, 10);
    }
}

#[test]
fn settlement_sync_failure_through_real_broker_path_withholds_response_and_does_not_retry() {
    for phase in [Phase::Written, Phase::FileSynced, Phase::DirectorySynced] {
        let directory = private();
        let mut ledger = Ledger::create(root(directory.path()), policy()).unwrap();
        ledger.test_fault = Some((2, phase));
        let mut b = Broker::new(ledger, credential(), FixtureTransport).unwrap();
        let reply = exchange(&mut b, &request(1));
        assert_eq!(reply.status, ReplyStatus::AccountingUncertain);
        assert!(reply.text.is_none() && b.report().halted && b.report().accounting_uncertain);
        assert_eq!(b.report().reserved_cost_microusd, 10);
        drop(b);
        let mut b = broker(directory.path());
        assert_eq!(exchange(&mut b, &request(1)).status, ReplyStatus::AlreadyFinished);
        assert_eq!(b.report().reserved_calls, 1);
    }
}


#[test]
fn admission_revocation_before_reservation_never_spends_or_invokes() {
    let directory = private();
    let ledger = Ledger::create(root(directory.path()), policy()).unwrap();
    let mut b = Broker::new(ledger, credential(), FixtureTransport).unwrap();
    let (reply, service) = b.process_guarded(&request(1), |_| false);
    assert_eq!(reply.status, ReplyStatus::Revoked);
    assert!(!service.provider_called && !service.outcome_recorded && reply.text.is_none());
    assert_eq!(b.report().reserved_calls, 0);
}

#[test]
fn revocation_after_reservation_retains_the_whole_bound_and_halts_new_calls() {
    let directory = private();
    let ledger = Ledger::create(root(directory.path()), policy()).unwrap();
    let mut b = Broker::new(ledger, credential(), FixtureTransport).unwrap();
    let (reply, service) = b.process_guarded(&request(1), |phase| phase != AdmissionPhase::BeforeDispatch);
    assert_eq!(reply.status, ReplyStatus::Revoked);
    assert!(!service.provider_called && service.outcome_recorded && reply.text.is_none());
    assert_eq!(b.report().reserved_cost_microusd, 10);
    assert!(b.report().halted);
    drop(b);
    let mut b = broker(directory.path());
    assert_eq!(exchange(&mut b, &request(1)).status, ReplyStatus::OutcomeUnknown);
    assert_eq!(exchange(&mut b, &request(2)).status, ReplyStatus::Halted);
}

#[test]
fn revocation_before_reply_withholds_text_but_cannot_refund_or_repeat() {
    let directory = private();
    let ledger = Ledger::create(root(directory.path()), policy()).unwrap();
    let mut b = Broker::new(ledger, credential(), FixtureTransport).unwrap();
    let (reply, service) = b.process_guarded(&request(1), |phase| phase != AdmissionPhase::BeforeReply);
    assert_eq!(reply.status, ReplyStatus::Revoked);
    assert!(service.provider_called && service.outcome_recorded && reply.text.is_none());
    assert_eq!(b.report().reserved_cost_microusd, 10);
    assert_eq!(b.report().calls[0].state, CallState::Completed);
    drop(b);
    let mut b = broker(directory.path());
    assert_eq!(exchange(&mut b, &request(1)).status, ReplyStatus::AlreadyFinished);
}

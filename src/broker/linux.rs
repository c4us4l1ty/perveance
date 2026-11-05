use super::*;
use crate::worker_io::PrivateDirectory;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    os::{fd::AsRawFd, unix::net::UnixStream},
    time::{Duration, Instant},
};

const MAGIC: &[u8; 8] = b"PVBRK01\n";
const MAX_RECORD_BYTES: usize = 2048;
const MAX_JOURNAL_BYTES: u64 = 1024 * 1024;
const IO_DEADLINE: Duration = Duration::from_millis(1500);
const FIXTURE_CREDENTIAL: &[u8] = b"synthetic-offline-broker-credential";

fn digest(previous: &[u8; 32], payload: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"perveance-broker-journal-v1\0");
    hash.update(previous);
    hash.update((payload.len() as u32).to_be_bytes());
    hash.update(payload);
    hash.finalize().into()
}
fn hex_digest(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Record {
    Header { version: u32, scope_id: String, policy: Policy },
    Reserved { sequence: u32, id: String, fingerprint: String, cost_microusd: u64 },
    Finished { sequence: u32, id: String, state: CallState, actual_cost_microusd: Option<u64>, response_bytes: usize },
}
struct Entry {
    id: String,
    fingerprint: String,
    cost: u64,
    state: CallState,
    actual: Option<u64>,
}

/// Holds a nonblocking exclusive flock for its ENTIRE lifetime. Other cooperating
/// processes must fail busy, not use a stale counter or open an unlocked fallback.
/// One private root is one budget domain; copying/resetting roots is not allowed
/// to an untrusted worker and cannot create a global/provider-account spend cap.
pub struct Ledger {
    root: PrivateDirectory,
    file: File,
    scope_id: String,
    policy: Policy,
    entries: Vec<Entry>,
    chain: [u8; 32],
    records: u32,
    poisoned: bool,
    #[cfg(test)]
    test_fault: Option<(u32, Phase)>,
}

fn lock(file: &File) -> Result<(), String> {
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(fail("ledger is busy or exclusive locking is unavailable; no fallback"));
    }
    Ok(())
}

impl Ledger {
    pub fn create(root: PrivateDirectory, policy: Policy) -> Result<Self, String> {
        policy.validate()?;
        let mut random = [0u8; 16];
        if unsafe { libc::getrandom(random.as_mut_ptr().cast(), random.len(), 0) } != random.len() as isize {
            return Err(fail("cannot allocate private ledger scope"));
        }
        let scope_id = random.iter().map(|b| format!("{b:02x}")).collect::<String>();
        // O_EXCL: never adopt, truncate or replace an occupied object, even if
        // an earlier initialization failed. Inspect it explicitly instead.
        let mut file = root.create_private(LEDGER_FILE).map_err(|_| fail("cannot create new ledger; occupied names are never reset"))?;
        lock(&file)?;
        file.write_all(MAGIC).map_err(|_| fail("cannot initialize ledger; inspect before retrying"))?;
        let mut ledger = Self { root, file, scope_id: scope_id.clone(), policy: policy.clone(), entries: Vec::new(),
            chain: [0; 32], records: 0, poisoned: false, #[cfg(test)] test_fault: None };
        ledger.append(&Record::Header { version: VERSION, scope_id, policy })?;
        Ok(ledger)
    }

    pub fn open(root: PrivateDirectory) -> Result<Self, String> {
        let mut file = root.open_private_rw(LEDGER_FILE, MAX_JOURNAL_BYTES)
            .map_err(|_| fail("cannot open bounded private ledger"))?;
        lock(&file)?;
        let mut bytes = Vec::new();
        (&mut file).take(MAX_JOURNAL_BYTES + 1).read_to_end(&mut bytes).map_err(|_| fail("cannot read ledger"))?;
        if bytes.len() as u64 > MAX_JOURNAL_BYTES || !bytes.starts_with(MAGIC) {
            return Err(fail("invalid or oversized ledger; no reset or truncation"));
        }
        let mut offset = MAGIC.len();
        let mut chain = [0u8; 32];
        let mut records = Vec::new();
        while offset < bytes.len() {
            if records.len() > (2 * MAX_CALLS) as usize || bytes.len() - offset < 4 {
                return Err(fail("partial or excessive ledger records; no repair/replay"));
            }
            let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().expect("four bytes")) as usize;
            offset += 4;
            if length == 0 || length > MAX_RECORD_BYTES || bytes.len() - offset < length + 32 {
                return Err(fail("partial or oversized ledger record; no repair/replay"));
            }
            let payload = &bytes[offset..offset + length];
            let next = digest(&chain, payload);
            if bytes[offset + length..offset + length + 32] != next {
                return Err(fail("ledger checksum/sequence failed; no repair/replay"));
            }
            let record: Record = serde_json::from_slice(payload).map_err(|_| fail("invalid ledger JSON or schema"))?;
            records.push(record);
            chain = next;
            offset += length + 32;
        }
        let mut records = records.into_iter();
        let Some(Record::Header { version, scope_id, policy }) = records.next() else {
            return Err(fail("ledger header missing; no reset"));
        };
        if version != VERSION || !call_id(&scope_id) { return Err(fail("invalid ledger header")); }
        policy.validate()?;
        let mut ledger = Self { root, file, scope_id, policy, entries: Vec::new(), chain,
            records: 1, poisoned: false, #[cfg(test)] test_fault: None };
        for record in records {
            ledger.apply(&record)?;
            ledger.records += 1;
        }
        // Confirm any completely visible records after a previous uncertain
        // sync before allowing new dispatch. Partial tails never get discarded.
        ledger.file.sync_all().and_then(|_| ledger.root.sync().map_err(std::io::Error::other))
            .map_err(|_| fail("cannot confirm ledger durability; calls disabled"))?;
        ledger.file.seek(SeekFrom::End(0)).map_err(|_| fail("cannot position locked ledger"))?;
        Ok(ledger)
    }

    pub fn report(&self) -> LedgerReport {
        LedgerReport { version: VERSION, scope_id: self.scope_id.clone(), policy: self.policy.clone(),
            reserved_calls: self.entries.len() as u64, reserved_cost_microusd: self.reserved_cost(),
            calls: self.entries.iter().map(|entry| CallSummary { id: entry.id.clone(), state: entry.state,
                reserved_cost_microusd: entry.cost, actual_cost_microusd: entry.actual }).collect(),
            halted: self.halted(), accounting_uncertain: self.poisoned, autonomous_execution: false }
    }
    pub fn policy(&self) -> &Policy { &self.policy }
    fn reserved_cost(&self) -> u64 {
        // Validated on every addition, including replay. Never subtract usage.
        self.entries.iter().map(|entry| entry.cost).sum()
    }
    fn halted(&self) -> bool {
        self.poisoned || self.entries.iter().any(|entry| entry.state != CallState::Completed)
    }
    fn validate_record(&self, record: &Record) -> Result<(), String> {
        match record {
            Record::Reserved { sequence, id, fingerprint, cost_microusd } => {
                if *sequence != self.records || self.halted() || !call_id(id) || !hex_digest(fingerprint)
                    || self.entries.iter().any(|entry| &entry.id == id)
                    || *cost_microusd != self.policy.per_call_upper_cost_microusd
                    || self.entries.len() as u64 >= self.policy.max_calls
                    || self.reserved_cost().checked_add(*cost_microusd).is_none_or(|n| n > self.policy.max_cost_microusd) {
                    return Err(fail("invalid reservation sequence or budget"));
                }
            }
            Record::Finished { sequence, id, state, actual_cost_microusd, response_bytes } => {
                let entry = self.entries.last().ok_or_else(|| fail("unreserved settlement"))?;
                let valid = match state {
                    CallState::Completed => actual_cost_microusd.is_some_and(|n| n <= entry.cost)
                        && (1..=MAX_COMPLETION_BYTES).contains(response_bytes),
                    CallState::ProviderRejected => actual_cost_microusd.is_none_or(|n| n <= entry.cost) && *response_bytes == 0,
                    CallState::BillingBoundViolated => actual_cost_microusd.is_some_and(|n| n > entry.cost) && *response_bytes == 0,
                    CallState::OutcomeUnknown => actual_cost_microusd.is_none() && *response_bytes == 0,
                    CallState::ReservedUncertain => false,
                };
                if *sequence != self.records || &entry.id != id || entry.state != CallState::ReservedUncertain || !valid {
                    return Err(fail("invalid settlement sequence, state or usage"));
                }
            }
            Record::Header { .. } => return Err(fail("duplicate ledger header")),
        }
        Ok(())
    }
    fn apply(&mut self, record: &Record) -> Result<(), String> {
        self.validate_record(record)?;
        match record {
            Record::Reserved { id, fingerprint, cost_microusd, .. } => {
                self.entries.push(Entry { id: id.clone(), fingerprint: fingerprint.clone(), cost: *cost_microusd,
                    state: CallState::ReservedUncertain, actual: None });
            }
            Record::Finished { state, actual_cost_microusd, .. } => {
                let entry = self.entries.last_mut().expect("validated reservation");
                entry.state = *state;
                entry.actual = *actual_cost_microusd;
            }
            Record::Header { .. } => unreachable!("validated non-header record"),
        }
        Ok(())
    }

    fn append(&mut self, record: &Record) -> Result<(), String> {
        #[cfg(test)]
        let fault = self.test_fault.filter(|(sequence, _)| *sequence == self.records).map(|(_, phase)| phase);
        #[cfg(not(test))]
        let fault: Option<Phase> = None;
        self.append_with_hook(record, |at| if fault == Some(at) { Err(()) } else { Ok(()) })
    }
    fn append_with_hook(&mut self, record: &Record, mut hook: impl FnMut(Phase) -> Result<(), ()>) -> Result<(), String> {
        if self.poisoned { return Err(fail("accounting uncertain; this ledger handle is disabled")); }
        if self.records != 0 { self.validate_record(record)?; }
        let payload = serde_json::to_vec(record).map_err(|_| fail("cannot encode accounting record"))?;
        if payload.len() > MAX_RECORD_BYTES || self.records > (2 * MAX_CALLS) as u32 {
            return Err(fail("accounting record limit exceeded"));
        }
        let chain = digest(&self.chain, &payload);
        let result = (|| {
            self.file.write_all(&(payload.len() as u32).to_be_bytes())?;
            self.file.write_all(&payload)?;
            self.file.write_all(&chain)?;
            hook(Phase::Written).map_err(|_| std::io::Error::other("injected write uncertainty"))?;
            self.file.sync_all()?;
            hook(Phase::FileSynced).map_err(|_| std::io::Error::other("injected sync uncertainty"))?;
            self.root.sync().map_err(std::io::Error::other)?;
            hook(Phase::DirectorySynced).map_err(|_| std::io::Error::other("injected parent-sync uncertainty"))?;
            Ok::<_, std::io::Error>(())
        })();
        if result.is_err() {
            self.poisoned = true;
            return Err(fail("accounting write/sync uncertain; no dispatch, refund, truncation or automatic retry"));
        }
        self.chain = chain;
        self.records += 1;
        Ok(())
    }
    fn fingerprint(&self, request: &CallRequest) -> Result<String, String> {
        let bytes = serde_json::to_vec(request).map_err(|_| fail("cannot bind call request"))?;
        let mut hash = Sha256::new();
        hash.update(b"perveance-broker-request-v1\0");
        hash.update(self.scope_id.as_bytes());
        hash.update(bytes);
        Ok(format!("{:x}", hash.finalize()))
    }
    fn reserve(&mut self, request: &CallRequest) -> Result<Option<ReplyStatus>, String> {
        let fingerprint = self.fingerprint(request)?;
        if let Some(entry) = self.entries.iter().find(|entry| entry.id == request.id) {
            return Ok(Some(if entry.fingerprint != fingerprint { ReplyStatus::Conflict }
                else { match entry.state {
                    CallState::Completed => ReplyStatus::AlreadyFinished,
                    CallState::ReservedUncertain | CallState::OutcomeUnknown => ReplyStatus::OutcomeUnknown,
                    CallState::ProviderRejected => ReplyStatus::ProviderRejected,
                    CallState::BillingBoundViolated => ReplyStatus::BillingBoundViolated,
                } }));
        }
        if self.halted() { return Ok(Some(ReplyStatus::Halted)); }
        let cost = self.policy.per_call_upper_cost_microusd;
        if self.entries.len() as u64 >= self.policy.max_calls
            || self.reserved_cost().checked_add(cost).is_none_or(|n| n > self.policy.max_cost_microusd) {
            return Ok(Some(ReplyStatus::BudgetExhausted));
        }
        let record = Record::Reserved { sequence: self.records, id: request.id.clone(), fingerprint, cost_microusd: cost };
        // Validate in-memory transition before writing; publish the transition
        // only after all writes/syncs. On failure the handle is poisoned.
        self.append(&record)?;
        // append increments records; apply checks the sequence being applied.
        self.records -= 1;
        let applied = self.apply(&record);
        self.records += 1;
        applied?;
        Ok(None)
    }
    fn finish(&mut self, id: &str, state: CallState, actual: Option<u64>, response_bytes: usize) -> Result<(), String> {
        let record = Record::Finished { sequence: self.records, id: id.to_owned(), state,
            actual_cost_microusd: actual, response_bytes };
        self.append(&record)?;
        self.records -= 1;
        let applied = self.apply(&record);
        self.records += 1;
        applied
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase { Written, FileSynced, DirectorySynced }

impl Credential {
    pub fn from_private_file(root: &PrivateDirectory, relative: &str) -> Result<Self, String> {
        Self::from_bytes(root.read_private(relative, 512).map_err(|_| fail("cannot capture private broker credential"))?)
    }
}

pub struct Broker<T: Transport> { ledger: Ledger, credential: Credential, transport: T }
impl<T: Transport> Broker<T> {
    pub fn new(ledger: Ledger, credential: Credential, transport: T) -> Result<Self, String> {
        if transport.route() != ledger.policy.route { return Err(fail("trusted transport does not match ledger route")); }
        Ok(Self { ledger, credential, transport })
    }
    pub fn report(&self) -> LedgerReport { self.ledger.report() }

    fn process(&mut self, request: &CallRequest) -> (Reply, ServeReport) {
        let status_report = |reply: Reply, provider_called, outcome_recorded| {
            let report = ServeReport { status: reply.status, provider_called, outcome_recorded };
            (reply, report)
        };
        if request.validate(&self.ledger.policy).is_err()
            || self.credential.appears_in(request.prompt.as_bytes()) || self.credential.appears_in(request.id.as_bytes()) {
            return status_report(Reply::new(None, ReplyStatus::InvalidRequest), false, false);
        }
        match self.ledger.reserve(request) {
            Ok(Some(status)) => return status_report(Reply::new(Some(&request.id), status), false, false),
            Err(_) => return status_report(Reply::new(Some(&request.id), ReplyStatus::AccountingUncertain), false, false),
            Ok(None) => {}
        }
        // The private journal is durable BEFORE invoking trusted adapter code.
        // Never retry, including timeouts, lost replies or unknown billing.
        let result = self.transport.complete(&self.credential, ProviderCall { route: &self.ledger.policy.route,
            prompt: &request.prompt, max_output_tokens: request.max_output_tokens,
            upper_cost_microusd: self.ledger.policy.per_call_upper_cost_microusd });
        let (state, actual, text) = match result {
            Err(_) => (CallState::OutcomeUnknown, None, None),
            Ok(receipt) => match receipt.actual_cost_microusd {
                Some(cost) if cost > self.ledger.policy.per_call_upper_cost_microusd =>
                    (CallState::BillingBoundViolated, Some(cost), None),
                None => (CallState::OutcomeUnknown, None, None),
                Some(cost) if receipt.text.is_empty() || receipt.text.len() > MAX_COMPLETION_BYTES
                    || self.credential.appears_in(receipt.text.as_bytes()) =>
                    (CallState::ProviderRejected, Some(cost), None),
                Some(cost) => (CallState::Completed, Some(cost), Some(receipt.text)),
            },
        };
        if self.ledger.finish(&request.id, state, actual, text.as_ref().map_or(0, String::len)).is_err() {
            return status_report(Reply::new(Some(&request.id), ReplyStatus::AccountingUncertain), true, false);
        }
        let status = match state {
            CallState::Completed => ReplyStatus::Completed,
            CallState::ProviderRejected => ReplyStatus::ProviderRejected,
            CallState::BillingBoundViolated => ReplyStatus::BillingBoundViolated,
            _ => ReplyStatus::OutcomeUnknown,
        };
        let mut reply = Reply::new(Some(&request.id), status);
        reply.text = text;
        status_report(reply, true, true)
    }

    /// One framed exchange on an already connected capability socket. There is
    /// no pathname listener, bearer token in JSON, arbitrary URL/model selection
    /// or file tool. Read/write have absolute deadlines, not reset-per-byte
    /// timeouts. A trusted provider adapter's own duration is NOT bounded here.
    pub fn serve_once(&mut self, mut stream: UnixStream) -> Result<ServeReport, String> {
        stream.set_nonblocking(true).map_err(|_| fail("cannot configure broker channel"))?;
        let request = read_frame(&mut stream, MAX_REQUEST_FRAME, Instant::now() + IO_DEADLINE)
            .and_then(|bytes| serde_json::from_slice::<CallRequest>(&bytes).map_err(|_| fail("invalid scoped request JSON")));
        let (reply, report) = match request {
            Ok(request) => self.process(&request),
            Err(_) => (Reply::new(None, ReplyStatus::InvalidRequest), ServeReport {
                status: ReplyStatus::InvalidRequest, provider_called: false, outcome_recorded: false }),
        };
        let bytes = serde_json::to_vec(&reply).map_err(|_| fail("cannot encode bounded reply"))?;
        write_frame(&mut stream, &bytes, MAX_REPLY_FRAME, Instant::now() + IO_DEADLINE)?;
        Ok(report)
    }
}

fn wait(stream: &UnixStream, event: libc::c_short, deadline: Instant) -> Result<(), String> {
    loop {
        let remaining = deadline.checked_duration_since(Instant::now()).ok_or_else(|| fail("channel deadline expired"))?;
        let millis = remaining.as_millis().saturating_add(1).min(i32::MAX as u128) as i32;
        let mut fd = libc::pollfd { fd: stream.as_raw_fd(), events: event, revents: 0 };
        let result = unsafe { libc::poll(&mut fd, 1, millis) };
        if result > 0 && fd.revents & libc::POLLNVAL == 0 { return Ok(()); }
        if result < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted { continue; }
        return Err(fail("channel unavailable or deadline expired"));
    }
}
fn read_exact(stream: &mut UnixStream, bytes: &mut [u8], deadline: Instant) -> Result<(), String> {
    let mut offset = 0;
    while offset < bytes.len() {
        wait(stream, libc::POLLIN, deadline)?;
        match stream.read(&mut bytes[offset..]) {
            Ok(0) => return Err(fail("channel closed during frame")),
            Ok(n) => offset += n,
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted) => {}
            Err(_) => return Err(fail("cannot read channel")),
        }
    }
    Ok(())
}
fn write_all(stream: &mut UnixStream, bytes: &[u8], deadline: Instant) -> Result<(), String> {
    let mut offset = 0;
    while offset < bytes.len() {
        wait(stream, libc::POLLOUT, deadline)?;
        match stream.write(&bytes[offset..]) {
            Ok(0) => return Err(fail("channel closed during reply")),
            Ok(n) => offset += n,
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted) => {}
            Err(_) => return Err(fail("cannot write channel")),
        }
    }
    Ok(())
}
fn read_frame(stream: &mut UnixStream, limit: usize, deadline: Instant) -> Result<Vec<u8>, String> {
    let mut length = [0u8; 4];
    read_exact(stream, &mut length, deadline)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > limit { return Err(fail("frame size rejected before allocation")); }
    let mut bytes = vec![0; length];
    read_exact(stream, &mut bytes, deadline)?;
    Ok(bytes)
}
fn write_frame(stream: &mut UnixStream, bytes: &[u8], limit: usize, deadline: Instant) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > limit { return Err(fail("frame exceeds protocol limit")); }
    write_all(stream, &(bytes.len() as u32).to_be_bytes(), deadline)?;
    write_all(stream, bytes, deadline)
}

pub fn client_call(mut stream: UnixStream, request: &CallRequest) -> Result<Reply, String> {
    stream.set_nonblocking(true).map_err(|_| fail("cannot configure scoped client channel"))?;
    let bytes = serde_json::to_vec(request).map_err(|_| fail("cannot encode scoped request"))?;
    write_frame(&mut stream, &bytes, MAX_REQUEST_FRAME, Instant::now() + IO_DEADLINE)?;
    let bytes = read_frame(&mut stream, MAX_REPLY_FRAME, Instant::now() + IO_DEADLINE)?;
    let reply: Reply = serde_json::from_slice(&bytes).map_err(|_| fail("invalid broker reply"))?;
    if reply.version != VERSION || reply.id.as_ref().is_some_and(|id| id != &request.id)
        || (reply.id.is_none() && reply.status != ReplyStatus::InvalidRequest)
        || (reply.status == ReplyStatus::Completed) != reply.text.is_some()
        || reply.text.as_ref().is_some_and(|text| text.is_empty() || text.len() > MAX_COMPLETION_BYTES) {
        return Err(fail("reply binding, schema or size rejected"));
    }
    Ok(reply)
}

#[derive(Debug, Serialize)]
pub struct FixtureReport {
    pub reply: Reply,
    pub service: ServeReport,
    pub ledger: LedgerReport,
    pub fixture_only: bool,
    pub network_used: bool,
    pub autonomous_execution: bool,
}

/// Useful offline CLI/library round trip. No real key is accepted by this helper;
/// a fixed synthetic key stays on the server side of the socket pair.
pub fn fixture_call(root: PrivateDirectory, request_root: &PrivateDirectory, relative_request: &str) -> Result<FixtureReport, String> {
    let bytes = request_root.read_private(relative_request, MAX_REQUEST_FRAME as u64)
        .map_err(|_| fail("cannot capture private bounded call fixture"))?;
    let request: CallRequest = serde_json::from_slice(&bytes).map_err(|_| fail("invalid call fixture JSON or schema"))?;
    let ledger = Ledger::open(root)?;
    let credential = Credential::from_bytes(FIXTURE_CREDENTIAL.to_vec())?;
    let mut broker = Broker::new(ledger, credential, FixtureTransport)?;
    let (server, client) = UnixStream::pair().map_err(|_| fail("cannot create private broker channel"))?;
    let service = std::thread::spawn(move || {
        let result = broker.serve_once(server);
        (result, broker.report())
    });
    let reply = client_call(client, &request);
    let (result, ledger) = service.join().map_err(|_| fail("offline fixture service lost"))?;
    Ok(FixtureReport { reply: reply?, service: result?, ledger, fixture_only: true,
        network_used: false, autonomous_execution: false })
}

#[cfg(test)]
mod tests;

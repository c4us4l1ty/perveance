//! Scoped model-broker building blocks. The shipped transport is offline-only;
//! this is not an autonomy permit, paid-provider integration or OS key store.
//! A worker receives only a connected, bounded protocol stream, never a key or
//! ledger directory. Trusted transports and the host OS/account remain trusted.
use serde::{Deserialize, Serialize};
use std::fmt;

pub const VERSION: u32 = 1;
pub const MAX_CALLS: u64 = 128;
pub const MAX_PROMPT_BYTES: usize = 4000;
pub const MAX_COMPLETION_BYTES: usize = 8192;
pub const MAX_REQUEST_FRAME: usize = 8192;
pub const MAX_REPLY_FRAME: usize = 65536;
pub const LEDGER_FILE: &str = "model-budget.v1";

fn fail(message: &str) -> String { format!("model broker: {message}") }
fn identifier(text: &str, maximum: usize) -> bool {
    !text.is_empty() && text.len() <= maximum
        && text.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.:/".contains(&b))
}
fn call_id(text: &str) -> bool {
    text.len() == 32 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Selected by trusted host code, never accepted in the worker protocol. This
/// names a transport/model/billing contract, not a user-selected URL. An adapter
/// must independently prove its route, token limits and true upper charge bound.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub provider: String,
    pub model: String,
    pub billing_revision: String,
}
impl Route {
    pub fn fixture() -> Self {
        Self { provider: "offline-fixture".into(), model: "fixed-text-v1".into(), billing_revision: "synthetic-v1".into() }
    }
    fn validate(&self) -> Result<(), String> {
        if !identifier(&self.provider, 64) || !identifier(&self.model, 128) || !identifier(&self.billing_revision, 64) {
            return Err(fail("invalid trusted route binding"));
        }
        Ok(())
    }
}

/// Immutable per-ledger scope. Reservations, not reported actual usage, enforce
/// these ceilings. No refunds, policy replacement or automatic ledger reset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub route: Route,
    pub max_calls: u64,
    pub max_cost_microusd: u64,
    pub per_call_upper_cost_microusd: u64,
    pub max_output_tokens: u32,
}
impl Policy {
    pub fn fixture(max_calls: u64, max_cost_microusd: u64, per_call_upper_cost_microusd: u64) -> Self {
        Self { route: Route::fixture(), max_calls, max_cost_microusd,
            per_call_upper_cost_microusd, max_output_tokens: 1024 }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.route.validate()?;
        if self.max_calls > MAX_CALLS || self.per_call_upper_cost_microusd == 0
            || !(1..=4096).contains(&self.max_output_tokens) {
            return Err(fail("invalid call/cost bound or output-token policy"));
        }
        Ok(())
    }
}

/// Never Serialize/Clone/Display. Debug is explicitly redacted. The optional
/// private-file loader is host-side only; it is not a worker credential mount.
pub struct Credential(Vec<u8>);
impl Credential {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        let value = Self(bytes);
        if !(16..=512).contains(&value.0.len()) || !value.0.iter().all(|b| (0x21..=0x7e).contains(b)) {
            return Err(fail("invalid private credential format"));
        }
        Ok(value)
    }
    /// Only trusted in-process transport code may use this borrowing escape
    /// hatch. The broker never puts this value in protocol or journal records.
    pub fn with_exposed<T>(&self, use_key: impl FnOnce(&str) -> T) -> T {
        use_key(std::str::from_utf8(&self.0).expect("validated ASCII credential"))
    }
    fn appears_in(&self, bytes: &[u8]) -> bool {
        bytes.windows(self.0.len()).any(|part| part == self.0.as_slice())
    }
}
impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("Credential(REDACTED)") }
}
impl Drop for Credential {
    fn drop(&mut self) {
        // Best effort for this allocation only: not a guarantee against other
        // copies, compiler/runtime/transport buffers, swap, core dumps or host OS.
        for byte in &mut self.0 { unsafe { std::ptr::write_volatile(byte, 0); } }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallRequest {
    pub version: u32,
    pub id: String,
    pub prompt: String,
    pub max_output_tokens: u32,
}
impl CallRequest {
    pub fn validate(&self, policy: &Policy) -> Result<(), String> {
        if self.version != VERSION || !call_id(&self.id) || self.prompt.trim().is_empty()
            || self.prompt.len() > MAX_PROMPT_BYTES || self.max_output_tokens == 0
            || self.max_output_tokens > policy.max_output_tokens {
            return Err(fail("invalid scoped call request"));
        }
        Ok(())
    }
}
impl fmt::Debug for CallRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallRequest").field("id", &self.id).field("prompt", &"REDACTED")
            .field("max_output_tokens", &self.max_output_tokens).finish()
    }
}

pub struct ProviderCall<'a> {
    pub route: &'a Route,
    pub prompt: &'a str,
    pub max_output_tokens: u32,
    pub upper_cost_microusd: u64,
}
pub struct Receipt {
    pub text: String,
    pub actual_cost_microusd: Option<u64>,
}
#[derive(Clone, Copy, Debug)]
pub enum TransportFailure { OutcomeUnknown }

/// Trusted adapter seam, not implementable/configurable through worker JSON.
/// No retries are performed by the broker. A real adapter must disable hidden
/// redirects/retries, enforce its endpoint and verify billing/token bounds.
/// Only the fixed offline transport below is shipped; no paid route is enabled.
pub trait Transport {
    fn route(&self) -> Route;
    fn complete(&mut self, credential: &Credential, call: ProviderCall<'_>) -> Result<Receipt, TransportFailure>;
}

pub struct FixtureTransport;
impl Transport for FixtureTransport {
    fn route(&self) -> Route { Route::fixture() }
    fn complete(&mut self, credential: &Credential, call: ProviderCall<'_>) -> Result<Receipt, TransportFailure> {
        // Exercise the private borrowing path, without networking or echoing the
        // credential/prompt. Amounts are synthetic reservations, not real spend.
        credential.with_exposed(|key| { let _ = key.len(); });
        Ok(Receipt { text: "Fixed offline broker response; no provider called.".into(),
            actual_cost_microusd: Some(call.upper_cost_microusd / 2) })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplyStatus {
    Completed, AlreadyFinished, OutcomeUnknown, Conflict, BudgetExhausted,
    Halted, InvalidRequest, ProviderRejected, BillingBoundViolated, AccountingUncertain, Revoked,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub version: u32,
    pub id: Option<String>,
    pub status: ReplyStatus,
    pub text: Option<String>,
}
impl Reply {
    fn new(id: Option<&str>, status: ReplyStatus) -> Self {
        Self { version: VERSION, id: id.map(str::to_owned), status, text: None }
    }
}
impl fmt::Debug for Reply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reply").field("id", &self.id).field("status", &self.status)
            .field("text", &"REDACTED").finish()
    }
}

/// Host-side admission checkpoints. A worker cannot supply or bypass this
/// callback. False revokes dispatch/text, never a reservation or possible bill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionPhase { BeforeReservation, BeforeDispatch, BeforeReply }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallState { ReservedUncertain, Completed, ProviderRejected, BillingBoundViolated, OutcomeUnknown }
#[derive(Debug, Serialize)]
pub struct CallSummary {
    pub id: String,
    pub state: CallState,
    pub reserved_cost_microusd: u64,
    pub actual_cost_microusd: Option<u64>,
}
/// Intentionally no prompt, credential, request fingerprint or response content.
#[derive(Debug, Serialize)]
pub struct LedgerReport {
    pub version: u32,
    pub scope_id: String,
    pub policy: Policy,
    pub reserved_calls: u64,
    pub reserved_cost_microusd: u64,
    pub calls: Vec<CallSummary>,
    /// An incomplete/uncertain call, rejected response or billing breach latches
    /// this ledger closed for NEW calls. There is intentionally no reset API.
    pub halted: bool,
    /// True means a write/sync failed in this handle. Counts include only
    /// confirmed transitions; do not infer remaining authority from them.
    pub accounting_uncertain: bool,
    pub autonomous_execution: bool,
}
#[derive(Debug, Serialize)]
pub struct ServeReport {
    pub status: ReplyStatus,
    pub provider_called: bool,
    pub outcome_recorded: bool,
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{client_call, fixture_call, Broker, FixtureReport, Ledger};

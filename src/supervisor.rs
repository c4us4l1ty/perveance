//! Experimental, data-only task intake. No job, permit, execution, resource
//! enforcement or security boundary is created by this module.
use crate::Project;
use serde::{Deserialize, Serialize};
use std::{fs::File, io::Read, path::Path};

pub const INTAKE_VERSION: u32 = 1;
pub const MAX_REQUEST_BYTES: u64 = 32 * 1024;
pub const MAX_POLICY_BYTES: u64 = 4096;
pub const MAX_PROMPT_BYTES: usize = 4000;

/// Requested resources or trusted caller-supplied ceilings. These values are
/// declarations for intake, not measured usage or operating-system limits.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budgets {
    pub wall_seconds: u64,
    pub cpu_seconds: u64,
    pub memory_mib: u64,
    pub scratch_mib: u64,
    pub processes: u64,
    pub model_calls: u64,
    pub model_cost_microusd: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntakePolicy {
    pub version: u32,
    pub maximum: Budgets,
}

impl Default for IntakePolicy {
    fn default() -> Self {
        Self { version: INTAKE_VERSION, maximum: Budgets {
            wall_seconds: 300, cpu_seconds: 300, memory_mib: 512,
            scratch_mib: 256, processes: 16, model_calls: 10,
            model_cost_microusd: 1_000_000,
        }}
    }
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRequest {
    pub version: u32,
    pub base_revision: String,
    pub prompt: String,
    pub budgets: Budgets,
}

fn read_bounded<T: for<'de> Deserialize<'de>>(path: &Path, limit: u64, kind: &str) -> Result<T, String> {
    // Paths must be trusted. Like the existing project loader, this follows
    // symlinks and may block on special files; it is not an untrusted file broker.
    let file = File::open(path).map_err(|_| format!("cannot open {kind}"))?;
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).map_err(|_| format!("cannot read {kind}"))?;
    if bytes.len() as u64 > limit { return Err(format!("{kind} exceeds size limit")); }
    // Avoid echoing untrusted field names or prompt contents in errors.
    serde_json::from_slice(&bytes).map_err(|_| format!("invalid {kind} JSON or schema"))
}

impl TaskRequest {
    pub fn load(path: &Path) -> Result<Self, String> { read_bounded(path, MAX_REQUEST_BYTES, "task request") }
}
impl IntakePolicy {
    pub fn load(path: &Path) -> Result<Self, String> { read_bounded(path, MAX_POLICY_BYTES, "intake policy") }
}

/// A validation report only. No value of this type grants permission to run.
#[derive(Debug, Serialize)]
pub struct IntakeReport {
    pub status: &'static str,
    pub revision: String,
    pub requested: Budgets,
    pub maximum: Budgets,
    pub blocking_gates: &'static [&'static str],
    pub autonomous_execution: bool,
    pub budgets_enforced: bool,
    pub persistent_task_created: bool,
}

/// Validates one loaded project and request against trusted caller policy. Even
/// a valid request is blocked unconditionally: no evidence supplied by the
/// request, policy, or diagnostics can authorize execution through this API.
pub fn inspect_request(project: &Project, request: &TaskRequest, policy: &IntakePolicy) -> Result<IntakeReport, String> {
    if request.version != INTAKE_VERSION { return Err("unsupported task request version".into()); }
    if policy.version != INTAKE_VERSION { return Err("unsupported intake policy version".into()); }
    if request.prompt.trim().is_empty() || request.prompt.len() > MAX_PROMPT_BYTES {
        return Err("prompt must contain non-whitespace text and be at most 4000 UTF-8 bytes".into());
    }
    let b = &request.budgets;
    let m = &policy.maximum;
    for (name, requested, maximum, required) in [
        ("wall_seconds", b.wall_seconds, m.wall_seconds, true),
        ("cpu_seconds", b.cpu_seconds, m.cpu_seconds, true),
        ("memory_mib", b.memory_mib, m.memory_mib, true),
        ("scratch_mib", b.scratch_mib, m.scratch_mib, true),
        ("processes", b.processes, m.processes, true),
        ("model_calls", b.model_calls, m.model_calls, false),
        ("model_cost_microusd", b.model_cost_microusd, m.model_cost_microusd, false),
    ] {
        if required && requested == 0 { return Err(format!("{name} must be greater than zero")); }
        if requested > maximum { return Err(format!("{name} exceeds intake policy maximum")); }
    }
    project.validate()?;
    let revision = project.revision()?;
    if request.base_revision != revision { return Err("project revision does not match task request".into()); }
    Ok(IntakeReport {
        status: "blocked", revision, requested: b.clone(), maximum: m.clone(),
        blocking_gates: &["verified_sandbox", "resource_enforcement", "credential_broker", "compatible_harness", "independent_verifier", "safe_activation_and_recovery"],
        autonomous_execution: false, budgets_enforced: false, persistent_task_created: false,
    })
}

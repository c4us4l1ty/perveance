//! Resource-enforced Linux **fixed-code diagnostics**, not an autonomous runner.
//! A trusted caller must provision a private, delegated cgroup-v2 root. Nothing
//! in this module accepts a program, command, model output or executable path.
use crate::Project;
use serde::Serialize;
use std::{path::Path, sync::atomic::AtomicBool};

#[cfg(target_os = "linux")]
mod linux;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Limits {
    pub wall_seconds: u64,
    /// Aggregate CPU budget monitored by the independent watchdog, with sampling
    /// overshoot. The kernel separately enforces `cpu_percent` as a rate limit.
    pub cpu_seconds: u64,
    pub cpu_percent: u64,
    pub memory_mib: u64,
    pub scratch_mib: u64,
    /// Includes bubblewrap's helpers and every descendant/thread, not just Python.
    pub processes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self { wall_seconds: 10, cpu_seconds: 2, cpu_percent: 50,
            memory_mib: 64, scratch_mib: 4, processes: 12 }
    }
}

impl Limits {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value, min, max) in [
            ("wall_seconds", self.wall_seconds, 1, 30),
            ("cpu_seconds", self.cpu_seconds, 1, 30),
            ("cpu_percent", self.cpu_percent, 1, 100),
            ("memory_mib", self.memory_mib, 32, 512),
            ("scratch_mib", self.scratch_mib, 1, 64),
            ("processes", self.processes, 8, 32),
        ] {
            if !(min..=max).contains(&value) {
                return Err(format!("bounded worker {name} must be {min} to {max}"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode { Inspect, Wait, ForkTree, CpuBurn, MemoryFlood, PidFlood, ScratchFlood, OutputFlood, Fail }

impl Mode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "inspect" => Ok(Self::Inspect), "wait" => Ok(Self::Wait),
            "fork-tree" => Ok(Self::ForkTree), "cpu" => Ok(Self::CpuBurn),
            "memory" => Ok(Self::MemoryFlood), "processes" => Ok(Self::PidFlood),
            "scratch" => Ok(Self::ScratchFlood), "output" => Ok(Self::OutputFlood),
            "fail" => Ok(Self::Fail), _ => Err("unknown fixed bounded-worker diagnostic mode".into()),
        }
    }

    #[cfg(target_os = "linux")]
    fn argument(self) -> &'static str {
        match self {
            Self::Inspect => "inspect", Self::Wait => "wait", Self::ForkTree => "fork-tree",
            Self::CpuBurn => "cpu", Self::MemoryFlood => "memory", Self::PidFlood => "processes",
            Self::ScratchFlood => "scratch", Self::OutputFlood => "output", Self::Fail => "fail",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome { Completed, Cancelled, TimedOut, CpuBudget, MemoryLimit, ProcessLimit, ScratchLimit, OutputLimit, Failed }

#[derive(Clone, Debug, Serialize)]
pub struct Usage {
    pub cpu_microseconds: u64,
    pub cpu_throttled_periods: u64,
    pub cpu_throttled_microseconds: u64,
    pub memory_peak_bytes: u64,
    pub oom_kills: u64,
    pub process_limit_hits: u64,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub outcome: Outcome,
    pub limits: Limits,
    pub worker_started: bool,
    pub cgroup_limits_applied: bool,
    pub cgroup_removed: bool,
    pub usage: Option<Usage>,
    pub input_snapshot_sealed: bool,
    pub input_revision: Option<String>,
    pub source_files_mounted: bool,
    pub fixed_code_only: bool,
    pub autonomous_execution: bool,
    /// The mechanisms are real, but the complete product security gate is open.
    pub security_boundary_verified: bool,
}

impl Report {
    fn cancelled(limits: &Limits) -> Self {
        Self { outcome: Outcome::Cancelled, limits: limits.clone(), worker_started: false,
            cgroup_limits_applied: false, cgroup_removed: false, usage: None,
            input_snapshot_sealed: false, input_revision: None, source_files_mounted: false,
            fixed_code_only: true, autonomous_execution: false, security_boundary_verified: false }
    }
}

#[derive(Debug, Serialize)]
pub struct RecoveryReport {
    pub removed_cgroups: usize,
    pub autonomous_execution: bool,
    /// Deliberately does not guess which leftover files are safe to delete.
    pub staging_files_removed: bool,
}

/// Runs only an embedded diagnostic. Linux cgroup-v2 delegation, unprivileged
/// bubblewrap and a trusted system Python installation are mandatory. There is
/// no unconfined fallback. Source paths, directory ancestors and the same-user
/// supervisor remain trusted; this is not safe input brokerage or a gate permit.
#[cfg(target_os = "linux")]
pub fn run(project: &Project, trusted_parent: &Path, delegated_root: &Path, mode: Mode,
           limits: &Limits, cancelled: &AtomicBool) -> Result<Report, String> {
    linux::run(project, trusted_parent, delegated_root, mode, limits, cancelled)
}

/// Preferred descriptor-mediated entry: sealed stdin carries the full project;
/// no source/staging directory or source pathname is visible to the worker.
/// Still accepts embedded fixed diagnostics only, not generated code.
#[cfg(target_os = "linux")]
pub fn run_snapshot(snapshot: &crate::worker_io::ProjectSnapshot, delegated_root: &Path, mode: Mode,
                    limits: &Limits, cancelled: &AtomicBool) -> Result<Report, String> {
    linux::run_snapshot(snapshot, delegated_root, mode, limits, cancelled)
}

/// Explicit recovery kills and removes only this backend's strictly named leaf
/// cgroups. A cooperating live supervisor holds the root lock and blocks it.
#[cfg(target_os = "linux")]
pub fn recover(delegated_root: &Path) -> Result<RecoveryReport, String> { linux::recover(delegated_root) }

#[cfg(not(target_os = "linux"))]
pub fn run(_project: &Project, _trusted_parent: &Path, _delegated_root: &Path, _mode: Mode,
           _limits: &Limits, _cancelled: &AtomicBool) -> Result<Report, String> {
    Err("bounded worker diagnostics require Linux".into())
}

#[cfg(not(target_os = "linux"))]
pub fn recover(_delegated_root: &Path) -> Result<RecoveryReport, String> {
    Err("bounded worker recovery requires Linux".into())
}


/// Fixed offline broker/worker integration fixtures, never an arbitrary executor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrokerMode { Inspect, HoldBeforeDispatch, HoldInFlight }
impl BrokerMode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "inspect" => Ok(Self::Inspect), "hold-before-dispatch" => Ok(Self::HoldBeforeDispatch),
            "hold-in-flight" => Ok(Self::HoldInFlight), _ => Err("unknown fixed broker-worker mode".into()),
        }
    }
    #[cfg(target_os = "linux")]
    fn argument(self) -> &'static str {
        match self { Self::Inspect => "inspect", Self::HoldBeforeDispatch => "hold-before-dispatch",
            Self::HoldInFlight => "hold-in-flight" }
    }
}

/// In-memory diagnostic observation, not a durable job event or dispatch permit.
#[derive(Default)]
pub struct BrokerProgress { state: std::sync::atomic::AtomicU8 }
impl BrokerProgress {
    /// 0 = no reservation observed, 1 = reservation synced, 2 = adapter entered.
    pub fn phase(&self) -> u8 { self.state.load(std::sync::atomic::Ordering::SeqCst) }
}

#[derive(Debug, Serialize)]
pub struct BrokerWorkerReport {
    pub worker: Report,
    pub ledger: crate::broker::LedgerReport,
    pub exchanges: Vec<crate::broker::ServeReport>,
    pub adapter_invocations: u64,
    pub channel_error: bool,
    pub namespace_checks_verified: bool,
    pub synthetic_fixture_only: bool,
    pub provider_network_used: bool,
    pub autonomous_execution: bool,
}

/// Host broker outside the worker namespaces/cgroup; peer gets sealed stdin and
/// one connected duplex stdout socket, not credentials or ledger descriptors.
/// Uses fresh synthetic credentials/host sentinels and fixed code only. The
/// ledger must have the offline route, one call and a sufficient positive bound.
#[cfg(target_os = "linux")]
pub fn run_broker_fixture(snapshot: &crate::worker_io::ProjectSnapshot, delegated_root: &Path,
                          ledger_root: &Path, mode: BrokerMode, limits: &Limits, cancelled: &AtomicBool,
                          progress: &BrokerProgress) -> Result<BrokerWorkerReport, String> {
    linux::brokered::run(snapshot, delegated_root, ledger_root, mode, limits, cancelled, progress)
}

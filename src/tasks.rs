//! Persistent jobs for existing, data-only raster proposals, not an autonomous
//! builder, credential broker, plugin runner or sandbox. A private, trusted local
//! Linux directory is required. No provider calls or generated code run here.

use serde::{Deserialize, Serialize};
use crate::{EditProposal, Project};
use std::path::Path;

pub const TASK_VERSION: u32 = 1;
pub const MAX_METADATA_BYTES: u64 = 4096;
pub const MAX_EVENTS: usize = 3;
pub const INPUT_PROJECT: &str = "input.perveance.json";
pub const INPUT_PROPOSAL: &str = "proposal.json";
pub const OUTPUT_PROJECT: &str = "output.perveance.json";
pub const OUTPUT_PNG: &str = "output.png";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState { Queued, Running, Succeeded, Failed, Cancelled, Interrupted }

impl TaskState {
    pub fn is_terminal(self) -> bool { !matches!(self, Self::Queued | Self::Running) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskReason { InvalidInputs, InputsChanged, OutputWriteFailed, OutputVerificationFailed, UserCancelled, WorkerLost }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    /// A fixed basename inside the private task directory, never a supplied path.
    pub file: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskOutputs {
    pub project: Artifact,
    pub png: Artifact,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskEvent {
    pub version: u32,
    pub sequence: usize,
    pub state: TaskState,
    pub reason: Option<TaskReason>,
    pub outputs: Option<TaskOutputs>,
}

/// Metadata only: never includes raw pixels, layer names, prompts or keys.
/// Running does not imply a live process: use explicit recovery after a crash.
#[derive(Debug, Serialize)]
pub struct TaskReport {
    pub version: u32,
    pub id: String,
    pub kind: &'static str,
    pub state: TaskState,
    pub sequence: usize,
    pub base_revision: String,
    pub expected_revision: String,
    pub cancel_requested: bool,
    pub history: Vec<TaskEvent>,
    pub outputs: Option<TaskOutputs>,
    /// Successful outputs were re-read and verified for this report. This is
    /// a snapshot check, not a lock against subsequent edits or authorization.
    pub artifacts_verified: bool,
    pub autonomous_execution: bool,
    pub network_used: bool,
    pub resource_quotas_enforced: bool,
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{cancel, create, inspect, recover, run};

#[cfg(not(target_os = "linux"))]
pub fn create(_root: &Path, _project: &Project, _proposal: &EditProposal) -> Result<TaskReport, String> {
    Err("persistent raster tasks require Linux".into())
}
#[cfg(not(target_os = "linux"))]
pub fn inspect(_root: &Path, _id: &str) -> Result<TaskReport, String> { Err("persistent raster tasks require Linux".into()) }
#[cfg(not(target_os = "linux"))]
pub fn run(_root: &Path, _id: &str) -> Result<TaskReport, String> { Err("persistent raster tasks require Linux".into()) }
#[cfg(not(target_os = "linux"))]
pub fn cancel(_root: &Path, _id: &str) -> Result<TaskReport, String> { Err("persistent raster tasks require Linux".into()) }
#[cfg(not(target_os = "linux"))]
pub fn recover(_root: &Path, _id: &str) -> Result<TaskReport, String> { Err("persistent raster tasks require Linux".into()) }

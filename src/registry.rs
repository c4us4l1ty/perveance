//! Quarantine-only capability storage. A manifest is a declaration; captured
//! digests and a host storage receipt are NOT independent verification or an
//! execution/installation permit. No loader, provider or activation exists here.
use crate::document::CapabilityRef;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const VERSION: u32 = 1;
pub const MAX_MANIFEST_BYTES: u64 = 16 * 1024;
pub const MAX_ARTIFACT_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 32;
pub const MAX_TOTAL_ARTIFACT_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_OPERATIONS: usize = 16;
pub const MAX_FUEL: u64 = 10_000_000;
pub const MAX_MEMORY_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: u64 = 1024 * 1024;
pub const MANIFEST_FILE: &str = "manifest.v1.json";
pub const ARTIFACT_FILE: &str = "artifact.wasm";
pub const RECEIPT_FILE: &str = "capture.v1.json";
const WASM_HEADER: &[u8] = b"\0asm\x01\0\0\0";
fn fail(message: &str) -> String { format!("capability quarantine: {message}") }
fn hash(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn symbol(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && (value.as_bytes()[0].is_ascii_alphabetic() || value.as_bytes()[0] == b'_')
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactFormat { WasmV1 }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactDeclaration { pub format: ArtifactFormat, pub bytes: u64 }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryPoints { pub validate_payload: String, pub apply_edits: String, pub render_layer: String }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationDeclaration { pub id: String, pub schema_version: u32 }
/// Bounded requests only. There is no runtime to enforce or grant these yet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeDeclaration { pub fuel: u64, pub memory_bytes: u64, pub output_bytes: u64 }
/// Candidate-supplied claims; no source/recipe is fetched or attested.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceDeclaration { pub source_sha256: String, pub recipe_sha256: String }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u32, pub reference: CapabilityRef, pub artifact: ArtifactDeclaration,
    pub document_version: u32, pub entrypoints: EntryPoints,
    pub operations: Vec<OperationDeclaration>, pub runtime: RuntimeDeclaration,
    pub provenance: ProvenanceDeclaration,
}
impl Manifest {
    pub fn validate(&self) -> Result<(), String> {
        self.reference.validate().map_err(|_| fail("invalid capability reference"))?;
        if self.version != VERSION || self.document_version == 0 || self.artifact.bytes < WASM_HEADER.len() as u64
            || self.artifact.bytes > MAX_ARTIFACT_BYTES || self.operations.is_empty() || self.operations.len() > MAX_OPERATIONS {
            return Err(fail("invalid manifest version, size or operation count"));
        }
        let mut names = BTreeSet::new();
        for name in [&self.entrypoints.validate_payload, &self.entrypoints.apply_edits, &self.entrypoints.render_layer] {
            if !symbol(name) || !names.insert(name) { return Err(fail("invalid or duplicate declared export name")); }
        }
        names.clear();
        for op in &self.operations {
            if !symbol(&op.id) || !names.insert(&op.id) || op.schema_version == 0 { return Err(fail("invalid or duplicate declared operation")); }
        }
        if self.runtime.fuel == 0 || self.runtime.fuel > MAX_FUEL || self.runtime.memory_bytes == 0
            || self.runtime.memory_bytes > MAX_MEMORY_BYTES || self.runtime.memory_bytes % 65536 != 0
            || self.runtime.output_bytes == 0 || self.runtime.output_bytes > MAX_OUTPUT_BYTES
            || !hex(&self.provenance.source_sha256, 64) || !hex(&self.provenance.recipe_sha256, 64) {
            return Err(fail("invalid bounded runtime or provenance declaration"));
        }
        Ok(())
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_MANIFEST_BYTES { return Err(fail("manifest exceeds byte limit")); }
        let manifest: Self = serde_json::from_slice(bytes).map_err(|_| fail("invalid manifest JSON or schema"))?;
        manifest.validate()?; Ok(manifest)
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| fail("cannot encode manifest"))?;
        if bytes.len() as u64 > MAX_MANIFEST_BYTES { return Err(fail("manifest exceeds byte limit")); }
        Ok(bytes)
    }
}

/// One immutable slot per exact (case-sensitive ID, version), not per artifact.
/// A different digest/ABI/schema/manifest cannot silently rebind that version.
/// The hash is a filename derivation, NOT a signature or authorization token.
pub fn slot_key(reference: &CapabilityRef) -> Result<String, String> {
    reference.validate().map_err(|_| fail("invalid exact reference"))?;
    let identity = serde_json::to_vec(&(&reference.id, &reference.version)).map_err(|_| fail("cannot encode slot identity"))?;
    let mut digest = Sha256::new(); digest.update(b"perveance-quarantine-slot-v1\0"); digest.update(identity);
    Ok(format!("{:x}", digest.finalize()))
}

/// Frozen host-owned bytes after bounded capture/hash binding. The name is
/// deliberately NOT VerifiedCapability: capture checks only the envelope/hash.
/// The separate static diagnostic grants no behavior/payload/provenance trust.
pub struct CapturedCandidate { manifest: Manifest, manifest_bytes: Vec<u8>, artifact: Vec<u8> }
impl CapturedCandidate {
    pub fn from_bytes(manifest: &[u8], artifact: &[u8]) -> Result<Self, String> {
        let manifest = Manifest::from_bytes(manifest)?;
        if artifact.len() as u64 > MAX_ARTIFACT_BYTES || artifact.len() as u64 != manifest.artifact.bytes
            || !artifact.starts_with(WASM_HEADER) || hash(artifact) != manifest.reference.artifact_sha256 {
            return Err(fail("artifact size, envelope or pinned digest mismatch"));
        }
        let manifest_bytes = manifest.canonical_bytes()?;
        Ok(Self { manifest, manifest_bytes, artifact: artifact.to_vec() })
    }
    pub fn manifest(&self) -> &Manifest { &self.manifest }
    /// Uncached diagnostic only; never creates an execution/activation permit.
    pub fn audit_static(&self) -> crate::module_audit::Report {
        crate::module_audit::inspect(&self.manifest, &self.manifest_bytes, &self.artifact)
    }
    /// Construct data-only frames after static checks; still no execution permit.
    pub fn prepare_call(&self, document: &crate::document::Document, intent: &crate::extension_protocol::Intent)
        -> Result<crate::extension_protocol::PreparedCall, crate::extension_protocol::Error> {
        self.prepare_call_in_scope(document,intent,"00000000000000000000000000000000")
    }
    pub(crate) fn prepare_call_in_scope(&self, document: &crate::document::Document, intent: &crate::extension_protocol::Intent,
        scope: &str) -> Result<crate::extension_protocol::PreparedCall, crate::extension_protocol::Error> {
        if !self.audit_static().static_profile_passed {return Err(crate::extension_protocol::Error::StaticProfileRequired);}
        crate::extension_protocol::PreparedCall::prepare(&self.manifest, &self.manifest_bytes, document, intent, scope)
    }
    #[cfg(target_os = "linux")]
    pub fn capture(root: &crate::worker_io::PrivateDirectory, manifest_name: &str, artifact_name: &str) -> Result<Self, String> {
        let manifest = root.read_private(manifest_name, MAX_MANIFEST_BYTES)?;
        let artifact = root.read_private(artifact_name, MAX_ARTIFACT_BYTES)?;
        Self::from_bytes(&manifest, &artifact)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub max_entries: usize, pub max_artifact_bytes: u64, pub max_total_artifact_bytes: u64,
    pub max_manifest_bytes: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct EntryReport {
    pub slot: String, pub reference: CapabilityRef, pub artifact_bytes: u64, pub manifest_sha256: String,
    pub declared_document_version: u32, pub declared_operations: Vec<OperationDeclaration>,
    pub declared_runtime: RuntimeDeclaration, pub document_version_supported: bool,
    pub state: &'static str, pub storage_integrity_checked: bool, pub wasm_envelope_only_checked: bool,
    pub provenance_verified: bool, pub capability_verified: bool, pub extension_abi_supported: bool,
    pub runtime_limits_enforced: bool, pub installed: bool, pub active: bool, pub available: bool,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub version: u32, pub scope: String, pub policy: Policy, pub entries: Vec<EntryReport>,
    pub total_artifact_bytes: u64, pub interrupted_stages: usize, pub mutations_blocked: bool,
    pub mode: &'static str, pub code_loaded: bool, pub provider_network_used: bool, pub autonomous_execution: bool,
}
#[derive(Debug, Serialize)]
pub struct ModuleAudit {
    pub registry_scope: String, pub slot: String, pub storage_integrity_checked: bool,
    pub audit: crate::module_audit::Report,
}
#[derive(Debug, Serialize)]
pub struct ProtocolInspection {
    pub registry_scope: String, pub slot: String, pub storage_integrity_checked: bool,
    pub call: crate::extension_protocol::CallReport, pub reply: Option<crate::extension_protocol::ReplyReport>,
}
#[derive(Debug, Serialize)]
pub struct DocumentBinding {
    pub layer_id: String, pub reference: CapabilityRef, pub slot: String, pub state: &'static str, pub available: bool,
}
#[derive(Debug, Serialize)]
pub struct DocumentResolution {
    pub registry_scope: String, pub document_id: String, pub document_revision: String,
    pub bindings: Vec<DocumentBinding>, pub mutations_blocked: bool,
    pub code_loaded: bool, pub provider_network_used: bool, pub autonomous_execution: bool,
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{Initialization, Quarantine, Registry};

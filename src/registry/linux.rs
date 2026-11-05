use super::*;
use crate::{document::{Content, Document}, worker_io::{Artifact, DirectoryPublication, FilePublication, PrivateDirectory}};
use std::{fs::File, os::fd::AsRawFd, path::Path};

const HEADER_FILE: &str = "registry.v1.json";
const LOCK_FILE: &str = "registry.lock";
const RECORD_LIMIT: u64 = 4096;
const PURPOSE: &str = "captured_quarantine_only_v1";
const FILES: [&str; 3] = [ARTIFACT_FILE, RECEIPT_FILE, MANIFEST_FILE];
const PACKAGE_LIMIT: u64 = MAX_ARTIFACT_BYTES + MAX_MANIFEST_BYTES + RECORD_LIMIT;
fn policy() -> Policy { Policy { max_entries: MAX_ENTRIES, max_artifact_bytes: MAX_ARTIFACT_BYTES,
    max_total_artifact_bytes: MAX_TOTAL_ARTIFACT_BYTES, max_manifest_bytes: MAX_MANIFEST_BYTES } }
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(value).map_err(|_| fail("cannot encode host storage record"))?;
    if bytes.len() as u64 > RECORD_LIMIT { return Err(fail("host storage record exceeds byte limit")); }
    Ok(bytes)
}
fn decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, String> {
    serde_json::from_slice(bytes).map_err(|_| fail("invalid host storage record JSON or schema"))
}
fn lock(file: &File) -> Result<(), String> {
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(fail("registry is already held or cannot be locked"));
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header { version: u32, purpose: String, scope: String, policy: Policy }
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    version: u32, purpose: String, registry_scope: String, slot: String,
    manifest_sha256: String, artifact_sha256: String, artifact_bytes: u64,
}
#[derive(Debug, Serialize)]
pub struct Initialization { pub registry: Report, pub publication: FilePublication }
#[derive(Debug, Serialize)]
pub struct Quarantine { pub admission: &'static str, pub entry: EntryReport, pub publication: DirectoryPublication, pub registry: Report }
fn entry_report(slot: &str, manifest: &Manifest, manifest_hash: String) -> EntryReport {
    EntryReport { slot: slot.into(), reference: manifest.reference.clone(), artifact_bytes: manifest.artifact.bytes,
        manifest_sha256: manifest_hash, declared_document_version: manifest.document_version,
        declared_operations: manifest.operations.clone(), declared_runtime: manifest.runtime.clone(),
        document_version_supported: manifest.document_version == crate::document::VERSION,
        state: "quarantined_unverified", storage_integrity_checked: true, wasm_envelope_only_checked: true,
        provenance_verified: false, capability_verified: false, extension_abi_supported: false,
        runtime_limits_enforced: false, installed: false, active: false, available: false }
}
struct Stored { manifest: Manifest, report: EntryReport, artifacts: Vec<Artifact> }
struct Inventory { entries: Vec<Stored>, total: u64, interrupted: usize }

/// Host-owned lifetime lock; never export this object, its FDs or root to a
/// worker. Mutable admission prevents concurrent calls sharing one lock object.
/// Separate instances/processes are excluded by the real flock.
pub struct Registry { root: PrivateDirectory, _lock: File, header: Header }
impl Registry {
    pub fn initialize(path: &Path) -> Result<Initialization, String> {
        let root = PrivateDirectory::open(path)?;
        if !root.list_names(1)?.is_empty() { return Err(fail("initialization requires an empty private root; existing storage is never reset")); }
        let held = root.create_private(LOCK_FILE)?; lock(&held)?;
        held.sync_all().map_err(|_| fail("cannot sync registry lock inode"))?; root.sync()?;
        let mut random = [0u8;16];
        if unsafe { libc::getrandom(random.as_mut_ptr().cast(), random.len(), 0) } != random.len() as isize {
            return Err(fail("cannot allocate registry scope; incomplete initialization must be retained"));
        }
        let header = Header { version: VERSION, purpose: PURPOSE.into(), scope: random.iter().map(|b| format!("{b:02x}")).collect(), policy: policy() };
        let publication = root.publish_bytes(HEADER_FILE, &encode(&header)?, RECORD_LIMIT)?;
        let registry = Self { root, _lock: held, header };
        // No fallible filesystem work after publication: a visible committed
        // header must never be misreported as a precommit-looking failure.
        Ok(Initialization { registry: registry.report(Inventory { entries: Vec::new(), total: 0, interrupted: 0 }), publication })
    }
    pub fn open(path: &Path) -> Result<Self, String> {
        let root = PrivateDirectory::open(path)?;
        let held = root.open_private_rw(LOCK_FILE, 0)?; lock(&held)?;
        let bytes = root.read_private(HEADER_FILE, RECORD_LIMIT).map_err(|_| fail("registry initialization incomplete or header inaccessible; never reset automatically"))?;
        let header: Header = decode(&bytes)?;
        if header.version != VERSION || header.purpose != PURPOSE || !hex(&header.scope,32)
            || header.policy != policy() || encode(&header)? != bytes {
            return Err(fail("invalid or incompatible immutable registry header"));
        }
        Ok(Self { root, _lock: held, header })
    }
    fn stored(&self, slot: &str) -> Result<Stored, String> {
        let directory = self.root.child(slot)?;
        let names = directory.list_names(FILES.len())?;
        if names.iter().map(String::as_str).ne(FILES) { return Err(fail("package members are incomplete or unrecognized")); }
        let manifest_bytes = directory.read_private(MANIFEST_FILE, MAX_MANIFEST_BYTES)?;
        let artifact = directory.read_private(ARTIFACT_FILE, MAX_ARTIFACT_BYTES)?;
        let candidate = CapturedCandidate::from_bytes(&manifest_bytes, &artifact)?;
        let bytes = directory.read_private(RECEIPT_FILE, RECORD_LIMIT)?;
        let receipt: Receipt = decode(&bytes)?;
        let manifest_hash = hash(&manifest_bytes);
        if manifest_bytes != candidate.manifest_bytes || slot_key(&candidate.manifest.reference)? != slot
            || receipt.version != VERSION || receipt.purpose != PURPOSE || receipt.registry_scope != self.header.scope
            || receipt.slot != slot || receipt.manifest_sha256 != manifest_hash
            || receipt.artifact_sha256 != candidate.manifest.reference.artifact_sha256
            || receipt.artifact_bytes != artifact.len() as u64 || encode(&receipt)? != bytes {
            return Err(fail("stored package binding or host capture receipt mismatch"));
        }
        let manifest = candidate.manifest;
        let report = entry_report(slot, &manifest, manifest_hash.clone());
        let artifacts = vec![Artifact { file: MANIFEST_FILE, bytes: manifest_bytes.len() as u64, sha256: manifest_hash },
            Artifact { file: ARTIFACT_FILE, bytes: artifact.len() as u64, sha256: hash(&artifact) },
            Artifact { file: RECEIPT_FILE, bytes: bytes.len() as u64, sha256: hash(&bytes) }];
        Ok(Stored { manifest, report, artifacts })
    }
    fn inventory(&self) -> Result<Inventory, String> {
        let names = self.root.list_names(MAX_ENTRIES+10)?;
        if !names.iter().any(|n| n==HEADER_FILE) || !names.iter().any(|n| n==LOCK_FILE) { return Err(fail("registry storage incomplete")); }
        let mut entries = Vec::new(); let mut total=0u64; let mut interrupted=0;
        for name in names {
            if name==HEADER_FILE || name==LOCK_FILE { continue; }
            if [".perveance-publish-", ".perveance-file-"].iter().any(|prefix| name.strip_prefix(prefix).is_some_and(|suffix| hex(suffix,32))) {
                interrupted+=1; continue;
            }
            if !hex(&name,64) || entries.len()==MAX_ENTRIES { return Err(fail("unrecognized registry entry or entry limit exceeded")); }
            let stored = self.stored(&name)?;
            total=total.checked_add(stored.report.artifact_bytes).ok_or_else(|| fail("storage counter overflow"))?;
            if total>MAX_TOTAL_ARTIFACT_BYTES { return Err(fail("registry artifact storage limit exceeded")); }
            entries.push(stored);
        }
        Ok(Inventory { entries, total, interrupted })
    }
    fn report(&self, inventory: Inventory) -> Report {
        Report { version: VERSION, scope: self.header.scope.clone(), policy: policy(), entries: inventory.entries.into_iter().map(|e| e.report).collect(),
            total_artifact_bytes: inventory.total, interrupted_stages: inventory.interrupted, mutations_blocked: inventory.interrupted!=0,
            mode: "quarantine_only_no_execution", code_loaded: false, provider_network_used: false, autonomous_execution: false }
    }
    pub fn inspect(&self) -> Result<Report, String> { Ok(self.report(self.inventory()?)) }
    pub fn quarantine(&mut self, candidate: &CapturedCandidate) -> Result<Quarantine, String> {
        let mut inventory = self.inventory()?;
        if inventory.interrupted!=0 { return Err(fail("interrupted private stages block admission; retain data for operator review, never auto-clean or replay")); }
        let slot = slot_key(&candidate.manifest.reference)?;
        if let Some(existing) = inventory.entries.iter().find(|entry| entry.report.slot==slot) {
            if existing.manifest != candidate.manifest { return Err(fail("immutable capability version already bound to a different manifest or pin")); }
            let parent_directory_synced = self.root.child(&slot)?.sync().is_ok() && self.root.sync().is_ok();
            let publication = DirectoryPublication { directory: slot, artifacts: existing.artifacts.clone(), committed: true,
                parent_directory_synced, autonomous_execution: false };
            return Ok(Quarantine { admission: "already_quarantined", entry: existing.report.clone(), publication, registry: self.report(inventory) });
        }
        if inventory.entries.len()>=MAX_ENTRIES || inventory.total.checked_add(candidate.artifact.len() as u64).is_none_or(|n| n>MAX_TOTAL_ARTIFACT_BYTES) {
            return Err(fail("immutable registry storage budget exhausted; no eviction or reset is available"));
        }
        let receipt = Receipt { version: VERSION, purpose: PURPOSE.into(), registry_scope: self.header.scope.clone(), slot: slot.clone(),
            manifest_sha256: hash(&candidate.manifest_bytes), artifact_sha256: candidate.manifest.reference.artifact_sha256.clone(), artifact_bytes: candidate.artifact.len() as u64 };
        let receipt = encode(&receipt)?;
        let files: [(&'static str, &[u8]);3] = [(MANIFEST_FILE, &candidate.manifest_bytes), (ARTIFACT_FILE, &candidate.artifact), (RECEIPT_FILE, &receipt)];
        let entry = entry_report(&slot, &candidate.manifest, hash(&candidate.manifest_bytes));
        let artifacts = files.iter().map(|(file, bytes)| Artifact { file: *file, bytes: bytes.len() as u64, sha256: hash(bytes) }).collect();
        inventory.total += candidate.artifact.len() as u64;
        inventory.entries.push(Stored { manifest: candidate.manifest.clone(), report: entry.clone(), artifacts });
        inventory.entries.sort_by(|a,b| a.report.slot.cmp(&b.report.slot));
        let registry = self.report(inventory);
        // All validation/report preparation precedes commit. After commit only
        // the publisher's honest parent-sync receipt is returned; no readback
        // error can disguise an already-visible package as an absent output.
        let publication = self.root.publish_files(&slot, &files, PACKAGE_LIMIT)?;
        Ok(Quarantine { admission: "quarantined", entry, publication, registry })
    }
    /// Diagnostic only: does NOT make a document renderable or consult/activate
    /// any runtime. Exact pins never fall back to another digest/version/schema.
    pub fn resolve_document(&self, document: &Document) -> Result<DocumentResolution, String> {
        document.validate().map_err(|_| fail("invalid document snapshot"))?;
        let inventory = self.inventory()?;
        let mut bindings = Vec::new();
        for layer in &document.layers {
            let Content::Capability { capability, .. } = &layer.content else { continue; };
            let slot = slot_key(capability)?;
            let state = match inventory.entries.iter().find(|entry| entry.report.slot==slot) {
                None => "missing",
                Some(entry) if &entry.manifest.reference!=capability => "pinned_reference_mismatch",
                Some(entry) if entry.manifest.document_version!=document.version => "incompatible_document_version",
                Some(_) => "quarantined_no_runtime",
            };
            bindings.push(DocumentBinding { layer_id: layer.id.clone(), reference: capability.clone(), slot, state, available: false });
        }
        Ok(DocumentResolution { registry_scope: self.header.scope.clone(), document_id: document.id.clone(),
            document_revision: document.revision().map_err(|_| fail("cannot fingerprint document"))?, bindings,
            mutations_blocked: inventory.interrupted!=0, code_loaded: false, provider_network_used: false, autonomous_execution: false })
    }
}

//! Experimental interpreter diagnostics. These fixtures are deliberately tiny
//! stubs: none is a useful capability, and a pass/vs-fail distinction here is
//! engine bookkeeping, not behavior or provenance verification.
use perveance::{
    capability_runtime::{Outcome, RuntimeReport},
    document::{CapabilityRef, Content, Document, OpaquePayload},
    extension_protocol::{Error, Intent, Kind},
    registry::{self, ArtifactDeclaration, ArtifactFormat, CapturedCandidate, EntryPoints, Manifest,
        OperationDeclaration, ProvenanceDeclaration, RuntimeDeclaration},
};
use sha2::{Digest, Sha256};

fn uleb(mut value: u32) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let digit = (value & 0x7f) as u8;
        value >>= 7;
        out.push(if value == 0 { digit } else { digit | 0x80 });
        if value == 0 { return out; }
    }
}
fn sleb(mut value: i64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        let done = (value == 0 && byte & 0x40 == 0) || (value == -1 && byte & 0x40 != 0);
        out.push(if done { byte } else { byte | 0x80 });
        if done { return out; }
    }
}
fn section(id: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![id];
    out.extend(uleb(body.len() as u32));
    out.extend(body);
    out
}
fn text(name: &str) -> Vec<u8> {
    let mut out = uleb(name.len() as u32);
    out.extend(name.as_bytes());
    out
}
#[derive(Clone, Copy)]
enum Fixture { ZeroAllocator, Reply, Loop, Trap }
fn reply_bytes() -> Vec<u8> {
    let mut out = b"PVEXRP01".to_vec();
    out.extend([1, 0, 0, 0]);
    out.extend([0u8; 32]);
    out.extend([0u8; 4]);
    out
}
fn module(fixture: Fixture) -> Vec<u8> {
    let allocator: Vec<u8> = match fixture {
        Fixture::ZeroAllocator => vec![0x41, 0x00, 0x0b],
        _ => vec![0x41, 0x80, 0x04, 0x0b],
    };
    let entry: Vec<u8> = match fixture {
        Fixture::Reply => {
            let mut body = vec![0x42];
            body.extend(sleb((64i64 << 32) | 48));
            body.push(0x0b);
            body
        }
        Fixture::Loop => vec![0x03, 0x40, 0x0c, 0x00, 0x0b],
        Fixture::Trap => vec![0x00, 0x0b],
        Fixture::ZeroAllocator => vec![0x42, 0x00, 0x0b],
    };
    let mut out = b"\0asm\x01\0\0\0".to_vec();
    out.extend(section(1, &[0x02, 0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7e, 0x60, 0x01, 0x7f, 0x01, 0x7f]));
    out.extend(section(3, &[0x04, 0x01, 0x00, 0x00, 0x00]));
    out.extend(section(5, &[0x01, 0x01, 0x01, 0x01]));
    let mut exports = uleb(5);
    for (name, kind, index) in [("memory", 2u8, 0u32), ("perveance_alloc_v1", 0, 0),
        ("validate_payload_v1", 0, 1), ("apply_edits_v1", 0, 2), ("render_layer_v1", 0, 3)] {
        exports.extend(text(name));
        exports.push(kind);
        exports.extend(uleb(index));
    }
    out.extend(section(7, &exports));
    if matches!(fixture, Fixture::Reply) {
        let pixels = reply_bytes();
        let mut data = uleb(1);
        data.extend([0u8; 4]);
        data.push(0x41);
        data.extend(sleb(64));
        data.push(0x0b);
        data.extend(uleb(pixels.len() as u32));
        data.extend(pixels);
        out.extend(section(11, &data));
    }
    let mut code = uleb(4);
    for body in [allocator, entry.clone(), entry.clone(), entry] {
        let mut function = vec![0x00];
        function.extend(body);
        code.extend(uleb(function.len() as u32));
        code.extend(function);
    }
    out.extend(section(10, &code));
    out
}
fn manifest(bytes: &[u8]) -> Manifest {
    Manifest {
        version: 1,
        reference: CapabilityRef { id: "example.runtime".into(), version: "1.0.0".into(),
            artifact_sha256: format!("{:x}", Sha256::digest(bytes)), abi_version: 1, payload_schema: 1 },
        artifact: ArtifactDeclaration { format: ArtifactFormat::WasmV1, bytes: bytes.len() as u64 },
        document_version: 2,
        entrypoints: EntryPoints { validate_payload: "validate_payload_v1".into(),
            apply_edits: "apply_edits_v1".into(), render_layer: "render_layer_v1".into() },
        operations: vec![OperationDeclaration { id: "example_edit".into(), schema_version: 1 }],
        runtime: RuntimeDeclaration { fuel: 100_000, memory_bytes: 65536, output_bytes: registry::MAX_OUTPUT_BYTES },
        provenance: ProvenanceDeclaration { source_sha256: "a".repeat(64), recipe_sha256: "b".repeat(64) },
    }
}
fn document(m: &Manifest) -> Document {
    let mut doc = Document::from_bytes(include_bytes!("../examples/opaque-document-v2.json")).unwrap();
    if let Content::Capability { capability, .. } = &mut doc.layers[1].content {
        *capability = m.reference.clone();
    }
    doc
}
fn intent(doc: &Document, kind: Kind) -> Intent {
    Intent { version: 1, call_id: format!("{:032x}", 1), base_revision: doc.revision().unwrap(),
        layer_id: doc.layers[1].id.clone(), kind, edit: None }
}
fn invoke(fixture: Fixture) -> (RuntimeReport, Document) {
    let bytes = module(fixture);
    let m = manifest(&bytes);
    let candidate = CapturedCandidate::from_bytes(&m.canonical_bytes().unwrap(), &bytes).unwrap();
    let doc = document(&m);
    let report = candidate.invoke(&doc, &intent(&doc, Kind::ValidatePayload)).unwrap();
    (report, doc)
}
fn no_authority(report: &RuntimeReport) {
    let gates = &report.gates;
    assert!(!gates.host_imports_linked && !gates.os_confinement_verified);
    assert!(!gates.behavior_verified && !gates.payload_schemas_verified && !gates.provenance_verified);
    assert!(!gates.capability_verified && !gates.activation_permitted);
    assert!(!gates.installed && !gates.active && !gates.available);
    assert!(!gates.document_modified && !gates.registry_modified && !gates.png_published);
    assert!(!gates.provider_network_used && !gates.autonomous_execution);
    assert!(!gates.authenticated && !gates.replay_protection_enforced);
    assert!(gates.fuel_metering_enabled && gates.memory_limit_enabled);
    assert!(gates.output_limit_enabled && gates.deterministic_mode_enabled && gates.eager_translation);
}

#[test]
fn zero_allocator_is_refused_after_real_instantiation_and_execution() {
    let (report, _) = invoke(Fixture::ZeroAllocator);
    assert_eq!(report.outcome, Outcome::AllocationRejected);
    assert_eq!(report.engine, "wasmi-2.0.0");
    assert!(report.gates.engine_compiled && report.gates.module_instantiated && report.gates.code_executed);
    assert_eq!(report.instance_memory_bytes, Some(65536));
    assert!(report.reply.is_none());
    no_authority(&report);
}

#[test]
fn executed_reply_is_still_rejected_because_the_request_binding_is_wrong() {
    let (report, _) = invoke(Fixture::Reply);
    assert_eq!(report.outcome, Outcome::ReplyRejected);
    assert!(report.gates.code_executed);
    let reply = report.reply.expect("engine produced a reply");
    assert!(!reply.wire_accepted && !reply.memory_window_checked);
    assert_eq!(reply.issue, Some(Error::RequestMismatch));
    no_authority(&report);
}

#[test]
fn infinite_loop_is_stopped_by_fuel_and_not_replayed() {
    let (report, _) = invoke(Fixture::Loop);
    assert_eq!(report.outcome, Outcome::OutOfFuel);
    assert!(report.gates.code_executed);
    assert!(report.reply.is_none());
    assert!(report.fuel_remaining.unwrap() < report.fuel_declared);
    no_authority(&report);
}

#[test]
fn unreachable_instruction_is_a_deterministic_trap_not_a_capability() {
    let (report, _) = invoke(Fixture::Trap);
    assert_eq!(report.outcome, Outcome::Trapped);
    assert!(report.gates.code_executed && report.reply.is_none());
    no_authority(&report);
}

#[test]
fn invocation_is_deterministic_and_never_mutates_the_captured_document() {
    let bytes = module(Fixture::Reply);
    let m = manifest(&bytes);
    let candidate = CapturedCandidate::from_bytes(&m.canonical_bytes().unwrap(), &bytes).unwrap();
    let doc = document(&m);
    let request = intent(&doc, Kind::ValidatePayload);
    let before = doc.canonical_bytes().unwrap();
    let first = candidate.invoke(&doc, &request).unwrap();
    let second = candidate.invoke(&doc, &request).unwrap();
    assert_eq!(serde_json::to_value(&first).unwrap(), serde_json::to_value(&second).unwrap());
    assert_eq!(before, doc.canonical_bytes().unwrap());
    assert!(doc.render_raster().is_err());
    assert!(doc.layers[1].visible == false);
}

#[test]
fn engine_limits_do_not_become_verification_or_activation_authority() {
    let (report, _) = invoke(Fixture::Reply);
    let json = serde_json::to_string(&report).unwrap();
    for forbidden in ["\"behavior_verified\":true", "\"capability_verified\":true",
        "\"activation_permitted\":true", "\"available\":true", "\"png_published\":true",
        "\"document_modified\":true", "\"registry_modified\":true"] {
        assert!(!json.contains(forbidden), "{forbidden}");
    }
    assert!(json.contains("\"host_imports_linked\":false"));
    assert!(json.contains("\"engine_compiled\":true"));
}

#[test]
fn missing_static_profile_refusal_precedes_any_instantiation() {
    let bytes = b"\0asm\x01\0\0\0";
    let mut m = manifest(bytes);
    m.artifact.bytes = 8;
    let candidate = CapturedCandidate::from_bytes(&m.canonical_bytes().unwrap(), bytes).unwrap();
    let doc = document(&m);
    assert_eq!(candidate.invoke(&doc, &intent(&doc, Kind::ValidatePayload)).unwrap_err(),
        Error::StaticProfileRequired);
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use perveance::registry::{Registry, RECEIPT_FILE};
    use std::{fs, os::unix::fs::PermissionsExt};
    fn private() -> tempfile::TempDir {
        tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o700)).tempdir().unwrap()
    }
    #[test]
    fn registry_invocation_is_read_only_exact_and_keeps_the_document_unrenderable() {
        let root = private();
        Registry::initialize(root.path()).unwrap();
        let bytes = module(Fixture::Reply);
        let m = manifest(&bytes);
        let doc = document(&m);
        let request = intent(&doc, Kind::ValidatePayload);
        let mut registry = Registry::open(root.path()).unwrap();
        let admitted = registry.quarantine(&CapturedCandidate::from_bytes(&m.canonical_bytes().unwrap(), &bytes).unwrap()).unwrap();
        let receipt = root.path().join(&admitted.entry.slot).join(RECEIPT_FILE);
        let before = fs::read(&receipt).unwrap();
        let document_before = doc.canonical_bytes().unwrap();
        let report = registry.invoke_capability(&admitted.entry.slot, &doc, &request).unwrap();
        assert!(report.storage_integrity_checked && report.slot == admitted.entry.slot);
        assert_eq!(report.report.outcome, Outcome::ReplyRejected);
        no_authority(&report.report);
        assert_eq!(before, fs::read(&receipt).unwrap());
        assert_eq!(document_before, doc.canonical_bytes().unwrap());
        assert!(doc.render_raster().is_err());
        assert!(registry.invoke_capability(&"0".repeat(64), &doc, &request).is_err());
        let stage = root.path().join(format!(".perveance-publish-{}", "9".repeat(32)));
        fs::create_dir(&stage).unwrap();
        assert!(registry.invoke_capability(&admitted.entry.slot, &doc, &request).is_err());
        assert!(stage.exists());
        assert_eq!(registry.resolve_document(&doc).unwrap().bindings[0].state, "quarantined_no_runtime");
    }
}

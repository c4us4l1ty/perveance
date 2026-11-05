use perveance::{document::CapabilityRef, registry::{self, ArtifactDeclaration, ArtifactFormat, CapturedCandidate, EntryPoints, Manifest, OperationDeclaration, ProvenanceDeclaration, RuntimeDeclaration}};
use serde_json::json;
use sha2::{Digest, Sha256};
fn artifact() -> Vec<u8> {
    // Deliberately unvalidated module contents. Header/hash matching must NOT
    // become a claim that exports, imports, behavior or provenance were tested.
    let mut bytes = b"\0asm\x01\0\0\0candidate-private-sentinel\0\xff".to_vec();
    bytes.extend(0..=255); bytes
}
fn manifest(bytes: &[u8], version: &str) -> Manifest {
    Manifest { version: 1, reference: CapabilityRef { id: "example.quarantined".into(), version: version.into(),
        artifact_sha256: format!("{:x}", Sha256::digest(bytes)), abi_version: 1, payload_schema: 1 },
        artifact: ArtifactDeclaration { format: ArtifactFormat::WasmV1, bytes: bytes.len() as u64 }, document_version: 2,
        entrypoints: EntryPoints { validate_payload: "validate_payload_v1".into(), apply_edits: "apply_edits_v1".into(), render_layer: "render_layer_v1".into() },
        operations: vec![OperationDeclaration { id: "example_edit".into(), schema_version: 1 }],
        runtime: RuntimeDeclaration { fuel: 100_000, memory_bytes: 65536, output_bytes: 4096 },
        provenance: ProvenanceDeclaration { source_sha256: "a".repeat(64), recipe_sha256: "b".repeat(64) } }
}
fn candidate(bytes: &[u8], version: &str) -> CapturedCandidate {
    CapturedCandidate::from_bytes(&manifest(bytes,version).canonical_bytes().unwrap(), bytes).unwrap()
}

#[test]
fn strict_manifest_round_trip_and_slots_bind_id_version_not_mutable_pins() {
    let original=manifest(&artifact(),"1.2.3");
    assert_eq!(Manifest::from_bytes(&original.canonical_bytes().unwrap()).unwrap(), original);
    let key=registry::slot_key(&original.reference).unwrap(); assert_eq!(key.len(),64);
    let mut reference=original.reference.clone(); reference.artifact_sha256="f".repeat(64); reference.abi_version=99;
    assert_eq!(registry::slot_key(&reference).unwrap(),key);
    reference.version="1.2.4".into(); assert_ne!(registry::slot_key(&reference).unwrap(),key);
    reference=original.reference.clone(); reference.id="Example.quarantined".into(); assert_ne!(registry::slot_key(&reference).unwrap(),key);
}
#[test]
fn candidate_bytes_require_exact_size_digest_and_only_the_supported_container_envelope() {
    let bytes=artifact(); let source=manifest(&bytes,"1.0.0"); let raw=source.canonical_bytes().unwrap();
    CapturedCandidate::from_bytes(&raw,&bytes).unwrap();
    let mut changed=bytes.clone(); changed[8]^=1; assert!(CapturedCandidate::from_bytes(&raw,&changed).is_err());
    assert!(CapturedCandidate::from_bytes(&raw,&bytes[..8]).is_err());
    let shell=b"#!/bin/sh\nexit 0\n"; let shell_manifest=manifest(shell,"1.0.0").canonical_bytes().unwrap();
    assert!(CapturedCandidate::from_bytes(&shell_manifest,shell).is_err());
    let mut header=bytes.clone(); header[4]=2; let other=manifest(&header,"1.0.0").canonical_bytes().unwrap();
    assert!(CapturedCandidate::from_bytes(&other,&header).is_err());
    let mut oversized=vec![0;registry::MAX_ARTIFACT_BYTES as usize+1]; oversized[..8].copy_from_slice(&bytes[..8]);
    assert!(CapturedCandidate::from_bytes(&serde_json::to_vec(&manifest(&oversized,"1.0.0")).unwrap(),&oversized).is_err());
}
#[test]
fn manifest_limits_and_declarations_cannot_request_host_authority_or_self_verification() {
    let original=manifest(&artifact(),"1.0.0"); let value=serde_json::to_value(&original).unwrap();
    for (key,val) in [("verified",json!(true)),("permissions",json!({"network":true})),("command",json!("candidate-private-sentinel")),("dependencies",json!(["https://host/private"]))] {
        let mut invalid=value.clone(); invalid[key]=val;
        let error=Manifest::from_bytes(&serde_json::to_vec(&invalid).unwrap()).unwrap_err(); assert!(!error.contains("sentinel"));
    }
    let mut invalid=original.clone(); invalid.version=2; assert!(invalid.validate().is_err());
    invalid=original.clone(); invalid.runtime.fuel=registry::MAX_FUEL+1; assert!(invalid.validate().is_err());
    invalid=original.clone(); invalid.runtime.memory_bytes=65537; assert!(invalid.validate().is_err());
    invalid=original.clone(); invalid.runtime.output_bytes=0; assert!(invalid.validate().is_err());
    invalid=original.clone(); invalid.entrypoints.render_layer="/bin/sh".into(); assert!(invalid.validate().is_err());
    invalid=original.clone(); invalid.entrypoints.render_layer=invalid.entrypoints.apply_edits.clone(); assert!(invalid.validate().is_err());
    invalid=original.clone(); invalid.operations.push(invalid.operations[0].clone()); assert!(invalid.validate().is_err());
    invalid=original.clone(); invalid.operations=(0..=registry::MAX_OPERATIONS).map(|i| OperationDeclaration { id:format!("op_{i}"),schema_version:1 }).collect(); assert!(invalid.validate().is_err());
    invalid=original.clone(); invalid.provenance.source_sha256="A".repeat(64); assert!(invalid.validate().is_err());
    invalid=original.clone(); invalid.document_version=0; assert!(invalid.validate().is_err());
    assert!(Manifest::from_bytes(&vec![b' ';registry::MAX_MANIFEST_BYTES as usize+1]).is_err());
    let mut duplicate=original.canonical_bytes().unwrap(); duplicate.pop(); duplicate.extend_from_slice(br#", "version":1}"#);
    assert!(Manifest::from_bytes(&duplicate).is_err());
}

#[cfg(target_os="linux")]
mod linux {
    use super::*;
    use perveance::{document::{Content, Document}, registry::Registry, worker_io::PrivateDirectory};
    use std::{fs, os::unix::fs::{PermissionsExt, symlink}, sync::{Arc, Barrier}, thread};
    fn private() -> tempfile::TempDir { tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o700)).tempdir().unwrap() }
    fn write(path: &std::path::Path, bytes: &[u8]) { fs::write(path,bytes).unwrap(); fs::set_permissions(path,fs::Permissions::from_mode(0o600)).unwrap(); }
    fn initialized() -> tempfile::TempDir { let root=private(); Registry::initialize(root.path()).unwrap(); root }
    fn doc(reference: &CapabilityRef) -> Document {
        let mut document=Document::from_bytes(include_bytes!("../examples/opaque-document-v2.json")).unwrap();
        if let Content::Capability { capability,.. }=&mut document.layers[1].content { *capability=reference.clone(); }
        document
    }
    #[test]
    fn initialize_is_private_bounded_nonresettable_and_has_a_real_lifetime_lock() {
        let root=private(); let init=Registry::initialize(root.path()).unwrap();
        assert!(init.publication.committed && init.publication.parent_directory_synced);
        assert!(init.registry.entries.is_empty() && !init.registry.code_loaded && !init.registry.autonomous_execution);
        assert_eq!(init.registry.scope.len(),32); assert_eq!(init.registry.policy.max_entries,registry::MAX_ENTRIES);
        assert!(Registry::initialize(root.path()).is_err());
        let first=Registry::open(root.path()).unwrap(); assert!(Registry::open(root.path()).is_err());
        assert_eq!(first.inspect().unwrap().scope,init.registry.scope); drop(first);
        Registry::open(root.path()).unwrap();
        for name in ["registry.lock","registry.v1.json"] { assert_eq!(fs::metadata(root.path().join(name)).unwrap().permissions().mode() & 0o777,0o600); }
    }
    #[test]
    fn capture_is_frozen_and_storage_deduplicates_exact_bytes_without_marking_them_available() {
        let root=initialized(); let input=private(); let bytes=artifact(); let manifest=manifest(&bytes,"1.0.0");
        write(&input.path().join("manifest.json"),&manifest.canonical_bytes().unwrap()); write(&input.path().join("module.wasm"),&bytes);
        let source=PrivateDirectory::open(input.path()).unwrap(); let captured=CapturedCandidate::capture(&source,"manifest.json","module.wasm").unwrap();
        write(&input.path().join("module.wasm"),b"source changed after capture"); write(&input.path().join("manifest.json"),b"source changed after capture");
        let mut registry=Registry::open(root.path()).unwrap(); let result=registry.quarantine(&captured).unwrap();
        assert_eq!(result.admission,"quarantined"); assert!(result.publication.committed && result.publication.parent_directory_synced);
        let package=root.path().join(&result.entry.slot); assert_eq!(fs::read(package.join(registry::ARTIFACT_FILE)).unwrap(),bytes);
        assert_eq!(fs::read_dir(&package).unwrap().count(),3);
        for name in [registry::ARTIFACT_FILE,registry::MANIFEST_FILE,registry::RECEIPT_FILE] { assert_eq!(fs::metadata(package.join(name)).unwrap().permissions().mode() & 0o777,0o600); }
        let entry=&result.entry; assert!(entry.storage_integrity_checked && entry.wasm_envelope_only_checked);
        assert!(!entry.capability_verified && !entry.provenance_verified && !entry.extension_abi_supported && !entry.runtime_limits_enforced && !entry.installed && !entry.active && !entry.available);
        let report=registry.inspect().unwrap(); let text=serde_json::to_string(&report).unwrap(); assert!(!text.contains("candidate-private-sentinel"));
        let receipt=fs::read(package.join(registry::RECEIPT_FILE)).unwrap(); assert!(!receipt.windows(b"candidate-private-sentinel".len()).any(|s|s==b"candidate-private-sentinel"));
        assert_eq!(registry.quarantine(&captured).unwrap().admission,"already_quarantined"); assert_eq!(fs::read_dir(root.path()).unwrap().count(),3);
        drop(registry); let reopened=Registry::open(root.path()).unwrap(); assert_eq!(reopened.inspect().unwrap().entries.len(),1);
        assert_eq!(reopened.inspect().unwrap().total_artifact_bytes,bytes.len() as u64);
    }
    #[test]
    fn version_rebinding_is_refused_but_distinct_versions_preserve_previous_packages() {
        let root=initialized(); let bytes=artifact(); let first=candidate(&bytes,"1.0.0"); let mut registry=Registry::open(root.path()).unwrap();
        let result=registry.quarantine(&first).unwrap(); let original=fs::read(root.path().join(&result.entry.slot).join(registry::MANIFEST_FILE)).unwrap();
        let mut replacement=manifest(&bytes,"1.0.0"); replacement.provenance.recipe_sha256="c".repeat(64);
        assert!(registry.quarantine(&CapturedCandidate::from_bytes(&replacement.canonical_bytes().unwrap(),&bytes).unwrap()).is_err());
        let mut different=bytes.clone(); different.push(3); assert!(registry.quarantine(&candidate(&different,"1.0.0")).is_err());
        assert_eq!(fs::read(root.path().join(&result.entry.slot).join(registry::MANIFEST_FILE)).unwrap(),original);
        registry.quarantine(&candidate(&different,"2.0.0")).unwrap(); let report=registry.inspect().unwrap(); assert_eq!(report.entries.len(),2);
        assert_eq!(report.total_artifact_bytes,(bytes.len()+different.len()) as u64);
    }
    #[test]
    fn exact_document_resolution_distinguishes_missing_wrong_pin_and_incompatible_declarations_without_rendering() {
        let root=initialized(); let output=private(); let bytes=artifact(); let first=candidate(&bytes,"1.0.0");
        let mut registry=Registry::open(root.path()).unwrap(); let document=doc(&first.manifest().reference);
        assert_eq!(registry.resolve_document(&document).unwrap().bindings[0].state,"missing");
        registry.quarantine(&first).unwrap(); let resolved=registry.resolve_document(&document).unwrap();
        assert_eq!(resolved.bindings[0].state,"quarantined_no_runtime"); assert!(!resolved.bindings[0].available && !resolved.code_loaded && !resolved.autonomous_execution);
        assert!(document.export_png(&PrivateDirectory::open(output.path()).unwrap(),"blocked.png").is_err()); assert_eq!(fs::read_dir(output.path()).unwrap().count(),0);
        let mut pin=first.manifest().reference.clone(); pin.payload_schema+=1;
        assert_eq!(registry.resolve_document(&doc(&pin)).unwrap().bindings[0].state,"pinned_reference_mismatch");
        let mut unknown=manifest(&bytes,"2.0.0"); unknown.document_version=3; unknown.reference.abi_version=99;
        let future=CapturedCandidate::from_bytes(&unknown.canonical_bytes().unwrap(),&bytes).unwrap(); registry.quarantine(&future).unwrap();
        assert_eq!(registry.resolve_document(&doc(&unknown.reference)).unwrap().bindings[0].state,"incompatible_document_version");
        assert!(!registry.inspect().unwrap().entries.iter().find(|entry|entry.reference.version=="2.0.0").unwrap().document_version_supported);
    }
    #[test]
    fn incomplete_initialization_or_unknown_members_fail_closed_without_repair_or_overwrite() {
        let incomplete=private(); write(&incomplete.path().join("registry.lock"),b"");
        assert!(Registry::open(incomplete.path()).is_err()); assert!(Registry::initialize(incomplete.path()).is_err()); assert_eq!(fs::read_dir(incomplete.path()).unwrap().count(),1);
        let root=initialized(); write(&root.path().join("unexpected-private-sentinel"),b"do not delete");
        assert!(Registry::open(root.path()).unwrap().inspect().is_err());
        assert_eq!(fs::read(root.path().join("unexpected-private-sentinel")).unwrap(),b"do not delete");
    }
    #[test]
    fn interrupted_stages_are_retained_block_admission_and_do_not_hide_committed_versions() {
        let root=initialized(); let bytes=artifact(); let first=candidate(&bytes,"1.0.0"); let mut registry=Registry::open(root.path()).unwrap(); registry.quarantine(&first).unwrap();
        let stage=root.path().join(format!(".perveance-publish-{}","0".repeat(32))); fs::create_dir(&stage).unwrap(); fs::set_permissions(&stage,fs::Permissions::from_mode(0o700)).unwrap(); write(&stage.join("partial"),b"retain uncertain data");
        let report=registry.inspect().unwrap(); assert_eq!(report.entries.len(),1); assert_eq!(report.interrupted_stages,1); assert!(report.mutations_blocked);
        assert!(registry.quarantine(&first).is_err()); assert!(registry.quarantine(&candidate(&bytes,"2.0.0")).is_err());
        assert!(stage.join("partial").exists()); assert_eq!(registry.resolve_document(&doc(&first.manifest().reference)).unwrap().bindings[0].state,"quarantined_no_runtime");
        drop(registry); assert!(Registry::open(root.path()).unwrap().inspect().unwrap().mutations_blocked);
    }
    #[test]
    fn tampered_artifacts_or_forged_behavior_receipts_never_become_usable() {
        let root=initialized(); let bytes=artifact(); let first=candidate(&bytes,"1.0.0"); let mut registry=Registry::open(root.path()).unwrap(); let slot=registry.quarantine(&first).unwrap().entry.slot;
        let path=root.path().join(&slot).join(registry::ARTIFACT_FILE); let mut corrupt=bytes.clone(); corrupt[8]^=1; write(&path,&corrupt);
        assert!(registry.inspect().is_err()); assert!(registry.quarantine(&candidate(&bytes,"2.0.0")).is_err());
        write(&path,&bytes); let receipt=root.path().join(slot).join(registry::RECEIPT_FILE); let mut value:serde_json::Value=serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap(); value["capability_verified"]=json!(true); write(&receipt,&serde_json::to_vec(&value).unwrap());
        assert!(registry.inspect().is_err());
    }
    #[test]
    fn private_capture_rejects_source_links_and_stored_packages_reject_extra_files() {
        let input=private(); let bytes=artifact(); write(&input.path().join("manifest.json"),&manifest(&bytes,"1.0.0").canonical_bytes().unwrap()); write(&input.path().join("module.wasm"),&bytes);
        symlink("module.wasm",input.path().join("linked.wasm")).unwrap(); let source=PrivateDirectory::open(input.path()).unwrap();
        assert!(CapturedCandidate::capture(&source,"manifest.json","linked.wasm").is_err()); assert!(CapturedCandidate::capture(&source,"../manifest.json","module.wasm").is_err());
        fs::hard_link(input.path().join("module.wasm"),input.path().join("hard.wasm")).unwrap(); assert!(CapturedCandidate::capture(&source,"manifest.json","module.wasm").is_err());
        let root=initialized(); let mut registry=Registry::open(root.path()).unwrap(); let slot=registry.quarantine(&candidate(&bytes,"1.0.0")).unwrap().entry.slot;
        write(&root.path().join(slot).join("unrecognized"),b"retain this data"); assert!(registry.inspect().is_err());
    }
    #[test]
    fn entry_budget_and_storage_budget_are_enforced_across_reopen_without_eviction() {
        let root=initialized(); let bytes=b"\0asm\x01\0\0\0"; let mut registry=Registry::open(root.path()).unwrap();
        for i in 0..registry::MAX_ENTRIES { registry.quarantine(&candidate(bytes,&format!("1.0.{i}"))).unwrap(); }
        assert!(registry.quarantine(&candidate(bytes,"2.0.0")).is_err()); drop(registry);
        assert_eq!(Registry::open(root.path()).unwrap().inspect().unwrap().entries.len(),registry::MAX_ENTRIES);
        let other=initialized(); let mut large=vec![0;registry::MAX_ARTIFACT_BYTES as usize]; large[..8].copy_from_slice(bytes);
        let mut registry=Registry::open(other.path()).unwrap();
        for i in 0..8 { registry.quarantine(&candidate(&large,&format!("1.0.{i}"))).unwrap(); }
        assert_eq!(registry.inspect().unwrap().total_artifact_bytes,registry::MAX_TOTAL_ARTIFACT_BYTES);
        assert!(registry.quarantine(&candidate(bytes,"2.0.0")).is_err()); assert_eq!(registry.inspect().unwrap().entries.len(),8);
    }
    #[test]
    fn immutable_header_policy_cannot_be_raised_and_receipts_are_bound_to_registry_scope() {
        let root=initialized(); let mut header:serde_json::Value=serde_json::from_slice(&fs::read(root.path().join("registry.v1.json")).unwrap()).unwrap(); header["policy"]["max_entries"]=json!(registry::MAX_ENTRIES+1);
        write(&root.path().join("registry.v1.json"),&serde_json::to_vec(&header).unwrap()); assert!(Registry::open(root.path()).is_err());
        let source=initialized(); let bytes=artifact(); let slot=Registry::open(source.path()).unwrap().quarantine(&candidate(&bytes,"1.0.0")).unwrap().entry.slot;
        let destination=initialized(); let imported=destination.path().join(&slot); fs::create_dir(&imported).unwrap(); fs::set_permissions(&imported,fs::Permissions::from_mode(0o700)).unwrap();
        for name in [registry::MANIFEST_FILE,registry::ARTIFACT_FILE,registry::RECEIPT_FILE] { write(&imported.join(name),&fs::read(source.path().join(&slot).join(name)).unwrap()); }
        assert!(Registry::open(destination.path()).unwrap().inspect().is_err());
    }
    #[test]
    fn concurrent_admissions_have_one_new_package_and_no_shared_directory_offsets() {
        let root=initialized(); let barrier=Arc::new(Barrier::new(4));
        let threads:Vec<_>=(0..4).map(|_| { let path=root.path().to_owned(); let barrier=Arc::clone(&barrier); thread::spawn(move || {
            let captured=candidate(&artifact(),"1.0.0"); barrier.wait(); Registry::open(&path).and_then(|mut registry|registry.quarantine(&captured))
        }) }).collect();
        let results:Vec<_>=threads.into_iter().map(|thread|thread.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|result|result.as_ref().is_ok_and(|result|result.admission=="quarantined")).count(),1);
        let registry=Arc::new(Registry::open(root.path()).unwrap()); let readers:Vec<_>=(0..4).map(|_| {let registry=Arc::clone(&registry);thread::spawn(move ||registry.inspect().unwrap().entries.len())}).collect();
        assert!(readers.into_iter().all(|thread|thread.join().unwrap()==1)); assert_eq!(fs::read_dir(root.path()).unwrap().count(),3);
    }
}

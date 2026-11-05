//! Hand-encoded byte fixtures, not an interpreter/compiler or a WAT dependency.
//! Even the passing fixtures are useless function stubs, NOT verified plugins.
use perveance::{document::CapabilityRef, module_audit::{self as audit, Rejection, Validation}, registry::{self, ArtifactDeclaration, ArtifactFormat, CapturedCandidate, EntryPoints, Manifest, OperationDeclaration, ProvenanceDeclaration, RuntimeDeclaration}};
use sha2::{Digest, Sha256};

fn uint(mut value:u32)->Vec<u8> {let mut b=Vec::new();loop{let n=(value&127)as u8;value>>=7;b.push(n|if value==0{0}else{128});if value==0{return b;}}}
fn sint(mut value:i32)->Vec<u8> {let mut b=Vec::new();loop{let n=(value&127)as u8;value>>=7;let done=(value==0 && n&64==0)||(value==-1 && n&64!=0);b.push(n|if done{0}else{128});if done{return b;}}}
fn text(value:&str)->Vec<u8>{let mut b=uint(value.len()as u32);b.extend_from_slice(value.as_bytes());b}
fn bodies(values:Vec<Vec<u8>>)->Vec<u8>{let mut b=uint(values.len()as u32);for body in values{b.extend(uint(body.len()as u32));b.extend(body);}b}
fn exports(values:Vec<(&str,u8,u32)>)->Vec<u8>{let mut b=uint(values.len()as u32);for(name,kind,index)in values{b.extend(text(name));b.push(kind);b.extend(uint(index));}b}
fn exported()->Vec<(&'static str,u8,u32)>{vec![("memory",2,0),(audit::ALLOC_EXPORT,0,0),("validate_payload_v1",0,1),("apply_edits_v1",0,2),("render_layer_v1",0,3)]}
fn stubs()->Vec<Vec<u8>>{vec![vec![0,0x41,0,0x0b],vec![0,0x42,0,0x0b],vec![0,0x42,0,0x0b],vec![0,0x42,0,0x0b]]}
#[derive(Clone)]
struct Module{sections:Vec<(u8,Vec<u8>)>}
impl Module{
    fn new()->Self{Self{sections:vec![
        (1,vec![2,0x60,2,0x7f,0x7f,1,0x7e,0x60,1,0x7f,1,0x7f]),
        (3,vec![4,1,0,0,0]),(5,vec![1,1,1,1]),(7,exports(exported())),(10,bodies(stubs()))]}}
    fn set(&mut self,id:u8,bytes:Vec<u8>){
        self.sections.retain(|(n,_)|*n!=id);self.sections.push((id,bytes));
        self.sections.sort_by_key(|(n,_)|match n{0=>255,12=>9,_=>*n});
    }
    fn bytes(&self)->Vec<u8>{let mut b=b"\0asm\x01\0\0\0".to_vec();for(id,data)in &self.sections{b.push(*id);b.extend(uint(data.len()as u32));b.extend(data);}b}
}
fn manifest(bytes:&[u8])->Manifest{Manifest{version:1,reference:CapabilityRef{id:"example.static_audit".into(),version:"1.0.0".into(),artifact_sha256:format!("{:x}",Sha256::digest(bytes)),abi_version:1,payload_schema:1},
    artifact:ArtifactDeclaration{format:ArtifactFormat::WasmV1,bytes:bytes.len()as u64},document_version:2,
    entrypoints:EntryPoints{validate_payload:"validate_payload_v1".into(),apply_edits:"apply_edits_v1".into(),render_layer:"render_layer_v1".into()},
    operations:vec![OperationDeclaration{id:"example_edit".into(),schema_version:1}],runtime:RuntimeDeclaration{fuel:100_000,memory_bytes:65536,output_bytes:4096},
    provenance:ProvenanceDeclaration{source_sha256:"a".repeat(64),recipe_sha256:"b".repeat(64)}}}
fn captured(bytes:&[u8],manifest:&Manifest)->CapturedCandidate{CapturedCandidate::from_bytes(&manifest.canonical_bytes().unwrap(),bytes).unwrap()}
fn report(module:&Module)->audit::Report{let bytes=module.bytes();captured(&bytes,&manifest(&bytes)).audit_static()}
fn no_authority(report:&audit::Report){
    assert!(!report.module_instantiated && !report.code_executed && !report.runtime_limits_enforced && !report.behavior_verified && !report.payload_schemas_verified
        && !report.provenance_verified && !report.capability_verified && !report.activation_permitted && !report.registry_modified && !report.installed && !report.active && !report.available && !report.autonomous_execution);
}
fn rejected(module:&Module,issue:Rejection)->audit::Report{let r=report(module);assert!(!r.static_profile_passed);assert_eq!(r.rejection,Some(issue));no_authority(&r);r}

#[test]
fn matching_static_signatures_are_not_behavior_payload_runtime_or_activation_verification(){
    let module=Module::new();let bytes=module.bytes();let manifest=manifest(&bytes);let r=captured(&bytes,&manifest).audit_static();
    assert!(r.static_profile_passed && r.counts_complete && r.abi_signatures_match);assert_eq!(r.structural_validation,Validation::Passed);assert_eq!(r.rejection,None);
    assert_eq!(r.reference,manifest.reference);assert_eq!(r.artifact_sha256,manifest.reference.artifact_sha256);
    assert_eq!(r.manifest_sha256,format!("{:x}",Sha256::digest(manifest.canonical_bytes().unwrap())));
    assert_eq!(r.counts.functions,4);assert_eq!(r.counts.code_bodies,4);assert_eq!(r.counts.exports,5);no_authority(&r);
}
#[test]
fn even_an_infinite_loop_is_only_inspected_never_executed_or_declared_safe(){
    let mut module=Module::new();let mut functions=stubs();functions[1]=vec![0,0x03,0x40,0x0c,0,0x0b,0x42,0,0x0b];module.set(10,bodies(functions));
    let r=report(&module);assert!(r.static_profile_passed);no_authority(&r);
}
#[test]
fn numeric_helpers_globals_and_unknown_schema_claims_do_not_prove_payload_support(){
    let mut module=Module::new();let mut types=vec![3,0x60,2,0x7f,0x7f,1,0x7e,0x60,1,0x7f,1,0x7f];types.extend([0x60,1,0x7d,1,0x7d]);module.set(1,types);
    module.set(3,vec![5,1,0,0,0,2]);let mut functions=stubs();functions.push(vec![0,0x20,0,0x0b]);module.set(10,bodies(functions));module.set(6,vec![1,0x7c,0,0x44,0,0,0,0,0,0,0,0,0x0b]);
    let bytes=module.bytes();let mut m=manifest(&bytes);m.reference.payload_schema=99;m.operations[0].schema_version=99;let r=captured(&bytes,&m).audit_static();assert!(r.static_profile_passed);no_authority(&r);
}
#[test]
fn audit_uses_frozen_bytes_and_is_repeatable_without_a_blessing_receipt(){
    let mut bytes=Module::new().bytes();let manifest=manifest(&bytes);let candidate=captured(&bytes,&manifest);
    bytes[8]^=255;let a=candidate.audit_static();let b=candidate.audit_static();
    assert_eq!(serde_json::to_value(&a).unwrap(),serde_json::to_value(&b).unwrap());assert!(a.static_profile_passed);no_authority(&a);
}
#[test]
fn unknown_manifest_abi_or_document_contracts_are_not_silently_treated_as_version_one(){
    let bytes=Module::new().bytes();for (abi,document) in [(99,2),(1,3)]{let mut m=manifest(&bytes);m.reference.abi_version=abi;m.document_version=document;
        let r=captured(&bytes,&m).audit_static();assert_eq!(r.rejection,Some(Rejection::UnsupportedManifestContract));assert_eq!(r.structural_validation,Validation::NotRun);assert!(!r.static_profile_passed);no_authority(&r);}
}
#[test]
fn a_header_is_not_a_signature_profile_and_all_truncated_fixture_prefixes_fail(){
    let bytes=Module::new().bytes();for end in 8..bytes.len(){let prefix=&bytes[..end];let r=captured(prefix,&manifest(prefix)).audit_static();assert!(!r.static_profile_passed,"prefix {end}");no_authority(&r);}
    let r=captured(&bytes[..8],&manifest(&bytes[..8])).audit_static();assert_eq!(r.structural_validation,Validation::Passed);assert_eq!(r.rejection,Some(Rejection::MemoryProfile));
}
#[test]
fn a_full_validator_rejects_type_stack_index_and_function_count_errors(){
    let mut module=Module::new();let mut functions=stubs();functions[0]=vec![0,0x42,0,0x0b];module.set(10,bodies(functions));
    assert_eq!(rejected(&module,Rejection::InvalidOrUnsupportedWasm).structural_validation,Validation::Failed);
    module=Module::new();let mut functions=stubs();functions[1]=vec![0,0x7c,0x0b];module.set(10,bodies(functions));rejected(&module,Rejection::InvalidOrUnsupportedWasm);
    module=Module::new();module.set(3,vec![4,1,0,0,127]);rejected(&module,Rejection::InvalidOrUnsupportedWasm);
    module=Module::new();module.set(10,bodies(stubs()[..3].to_vec()));let r=report(&module);assert!(!r.static_profile_passed);no_authority(&r);
    module=Module::new();let mut values=exported();values[2].2=99;module.set(7,exports(values));rejected(&module,Rejection::InvalidOrUnsupportedWasm);
}
#[test]
fn imports_start_tables_and_elements_are_forbidden_before_any_runtime_could_exist(){
    let mut module=Module::new();let mut import=vec![1];import.extend(text("candidate-private-sentinel"));import.extend(text("read_secret"));import.extend([0,0]);module.set(2,import);
    let r=rejected(&module,Rejection::ImportsForbidden);assert!(!serde_json::to_string(&r).unwrap().contains("candidate-private-sentinel"));
    module=Module::new();module.set(8,vec![0]);rejected(&module,Rejection::StartForbidden);
    module=Module::new();module.set(4,vec![1,0x70,1,1,1]);rejected(&module,Rejection::SectionForbidden);
    module=Module::new();module.set(9,vec![1,0,0x41,0,0x0b,0]);rejected(&module,Rejection::SectionForbidden);
    module=Module::new();module.set(13,vec![0]);rejected(&module,Rejection::SectionForbidden);
}
#[test]
fn memory_must_be_one_unshared_bounded_32_bit_memory_within_the_declaration(){
    for memory in [vec![1,0,1],vec![1,1,1,2],vec![1,1,2,1],vec![1,1,0,1],vec![1,3,1,1],vec![1,5,1,1],vec![2,1,1,1,1,1,1],vec![0]]{
        let mut module=Module::new();module.set(5,memory);rejected(&module,Rejection::MemoryProfile);
    }
    let mut module=Module::new();module.sections.retain(|(id,_)|*id!=5);let r=report(&module);assert!(!r.static_profile_passed);no_authority(&r);
}
#[test]
fn exports_are_exact_and_reserved_names_cannot_be_repurposed(){
    let mut module=Module::new();let mut values=exported();values[4].0="candidate_private_sentinel";module.set(7,exports(values));
    let r=rejected(&module,Rejection::AbiExports);assert_eq!(r.structural_validation,Validation::Passed);assert!(!serde_json::to_string(&r).unwrap().contains("candidate_private_sentinel"));
    module=Module::new();module.set(7,exports(exported()[..4].to_vec()));rejected(&module,Rejection::AbiExports);
    module=Module::new();let mut values=exported();values.push(("extra",0,1));module.set(7,exports(values));rejected(&module,Rejection::ResourceLimit);
    module=Module::new();let mut values=exported();values[4].0=values[3].0;module.set(7,exports(values));rejected(&module,Rejection::AbiExports);
    let bytes=Module::new().bytes();for name in [audit::ALLOC_EXPORT,audit::MEMORY_EXPORT]{let mut m=manifest(&bytes);m.entrypoints.render_layer=name.into();let r=captured(&bytes,&m).audit_static();assert_eq!(r.rejection,Some(Rejection::AbiExports));}
}
#[test]
fn structurally_valid_wrong_function_signatures_are_not_accepted(){
    let mut module=Module::new();module.set(1,vec![2,0x60,1,0x7f,1,0x7e,0x60,1,0x7f,1,0x7f]);
    let r=rejected(&module,Rejection::AbiSignatures);assert_eq!(r.structural_validation,Validation::Passed);assert!(!r.abi_signatures_match);
    module=Module::new();let mut values=exported();values[3].2=0;module.set(7,exports(values));rejected(&module,Rejection::AbiSignatures);
}
#[test]
fn reference_types_and_unselected_instruction_proposals_are_not_enabled_by_default(){
    let mut module=Module::new();module.set(1,vec![2,0x60,2,0x70,0x7f,1,0x7e,0x60,1,0x7f,1,0x7f]);rejected(&module,Rejection::TypeForbidden);
    module=Module::new();let mut functions=stubs();functions[1]=vec![0,0x20,0,0x20,1,0x12,1,0x0b];module.set(10,bodies(functions));rejected(&module,Rejection::InvalidOrUnsupportedWasm); // return_call
}
#[test]
fn type_function_global_export_and_parser_event_counts_are_bounded(){
    for (id,limit) in [(1,audit::MAX_TYPES),(3,audit::MAX_FUNCTIONS),(6,audit::MAX_GLOBALS),(7,audit::MAX_EXPORTS)]{let mut module=Module::new();module.set(id,uint(limit+1));rejected(&module,Rejection::ResourceLimit);}
    let mut module=Module::new();for _ in 0..audit::MAX_PARSER_EVENTS{module.sections.push((0,vec![0]));}rejected(&module,Rejection::ResourceLimit);
    module=Module::new();let mut types=vec![2,0x60];types.extend(uint(audit::MAX_PARAMS as u32+1));types.extend(vec![0x7f;audit::MAX_PARAMS+1]);types.extend([1,0x7e,0x60,1,0x7f,1,0x7f]);module.set(1,types);rejected(&module,Rejection::ResourceLimit);
    module=Module::new();let mut types=vec![2,0x60,2,0x7f,0x7f];types.extend(uint(audit::MAX_RESULTS as u32+1));types.extend(vec![0x7e;audit::MAX_RESULTS+1]);types.extend([0x60,1,0x7f,1,0x7f]);module.set(1,types);rejected(&module,Rejection::ResourceLimit);
}
#[test]
fn local_group_value_and_aggregate_counts_are_bounded_without_expansion(){
    let mut module=Module::new();let mut functions=stubs();functions[1]=uint(audit::MAX_LOCALS_PER_FUNCTION+1);functions[1].push(0x0b);module.set(10,bodies(functions));rejected(&module,Rejection::ResourceLimit);
    module=Module::new();let mut functions=stubs();let mut excessive=vec![1];excessive.extend(uint(u32::MAX));excessive.extend([0x7f,0x42,0,0x0b]);functions[1]=excessive;module.set(10,bodies(functions));rejected(&module,Rejection::ResourceLimit);
    module=Module::new();let count=audit::MAX_TOTAL_LOCALS/audit::MAX_LOCALS_PER_FUNCTION+1;let mut declarations=uint(count);declarations.push(1);declarations.extend(vec![0;count as usize-1]);module.set(3,declarations);
    let functions=(0..count).map(|index|{let mut body=vec![1];body.extend(uint(audit::MAX_LOCALS_PER_FUNCTION));body.extend([0x7f,if index==0{0x41}else{0x42},0,0x0b]);body}).collect();module.set(10,bodies(functions));rejected(&module,Rejection::ResourceLimit);
}
#[test]
fn code_body_instruction_depth_and_branch_table_sizes_are_bounded(){
    let mut module=Module::new();let mut functions=stubs();functions[0]=vec![1;audit::MAX_BODY_BYTES+1];module.set(10,bodies(functions));rejected(&module,Rejection::ResourceLimit);
    module=Module::new();let mut functions=stubs();let mut body=vec![0];body.extend(vec![1;audit::MAX_INSTRUCTIONS as usize]);body.extend([0x41,0,0x0b]);functions[0]=body;module.set(10,bodies(functions));rejected(&module,Rejection::ResourceLimit);
    module=Module::new();let mut functions=stubs();let mut body=vec![0];for _ in 0..audit::MAX_CONTROL_DEPTH{body.extend([2,0x40]);}body.extend(vec![0x0b;audit::MAX_CONTROL_DEPTH as usize]);body.extend([0x41,0,0x0b]);functions[0]=body;module.set(10,bodies(functions));rejected(&module,Rejection::ResourceLimit);
    module=Module::new();let mut functions=stubs();let mut body=vec![0,0x42,0,0x41,0,0x0e];body.extend(uint(audit::MAX_BRANCH_TARGETS+1));body.extend(vec![0;audit::MAX_BRANCH_TARGETS as usize+2]);body.extend([0x42,0,0x0b]);functions[1]=body;module.set(10,bodies(functions));rejected(&module,Rejection::ResourceLimit);
}
fn active(offset:i32,bytes:&[u8])->Vec<u8>{let mut data=vec![1,0,0x41];data.extend(sint(offset));data.push(0x0b);data.extend(uint(bytes.len()as u32));data.extend(bytes);data}
#[test]
fn data_counts_sizes_and_active_initializer_bounds_are_checked_but_never_executed(){
    let mut module=Module::new();module.set(11,active(0,b"candidate-private-sentinel"));let r=report(&module);assert!(r.static_profile_passed);assert!(!serde_json::to_string(&r).unwrap().contains("candidate-private-sentinel"));no_authority(&r);
    module=Module::new();module.set(11,active(65535,b"xx"));rejected(&module,Rejection::DataInitializer);
    module=Module::new();module.set(11,active(-1,b"x"));rejected(&module,Rejection::DataInitializer);
    module=Module::new();module.set(11,active(0,&vec![0;audit::MAX_DATA_BYTES+1]));rejected(&module,Rejection::ResourceLimit);
    for id in [11,12]{module=Module::new();module.set(id,uint(audit::MAX_DATA_SEGMENTS+1));rejected(&module,Rejection::ResourceLimit);}
    module=Module::new();module.set(11,vec![1,1,1,42]);let r=report(&module);assert!(r.static_profile_passed);no_authority(&r); // passive one-byte data
}
#[test]
fn custom_sections_are_bounded_and_never_echoed_into_diagnostics(){
    let mut module=Module::new();let mut custom=text("candidate-private-sentinel");custom.extend(b"source/source.map private contents");module.set(0,custom);let r=report(&module);assert!(r.static_profile_passed);assert!(!serde_json::to_string(&r).unwrap().contains("sentinel"));
    module=Module::new();module.set(0,vec![0;audit::MAX_CUSTOM_BYTES+1]);rejected(&module,Rejection::ResourceLimit);
    module=Module::new();let mut global=vec![1,0x7f,0];global.extend(vec![1;audit::MAX_INITIALIZER_BYTES+1]);global.extend([0x41,0,0x0b]);module.set(6,global);rejected(&module,Rejection::ResourceLimit);
}
#[test]
fn byte_mutation_corpus_never_gains_authority_even_when_some_mutations_remain_valid(){
    let bytes=Module::new().bytes();for index in 8..bytes.len(){let mut mutation=bytes.clone();mutation[index]^=0x80;let r=captured(&mutation,&manifest(&mutation)).audit_static();no_authority(&r);}
}

#[cfg(target_os="linux")]
mod linux {
    use super::*;
    use perveance::{document::{Content,Document},registry::Registry};
    use std::{collections::BTreeMap,fs,os::unix::fs::PermissionsExt,path::{Path,PathBuf}};
    fn private()->tempfile::TempDir{tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o700)).tempdir().unwrap()}
    fn snapshot(root:&Path)->BTreeMap<PathBuf,Vec<u8>>{let mut result=BTreeMap::new();for entry in fs::read_dir(root).unwrap(){let path=entry.unwrap().path();if path.is_dir(){for(p,b)in snapshot(&path){result.insert(p,b);}}else{let bytes=fs::read(&path).unwrap();result.insert(path,bytes);}}result}
    #[test]
    fn registry_audits_only_exact_stored_bytes_without_mutating_state_or_enabling_rendering(){
        let root=private();Registry::initialize(root.path()).unwrap();let bytes=Module::new().bytes();let manifest=manifest(&bytes);
        let mut registry=Registry::open(root.path()).unwrap();let stored=registry.quarantine(&captured(&bytes,&manifest)).unwrap();let before=snapshot(root.path());let inventory=serde_json::to_value(registry.inspect().unwrap()).unwrap();
        let result=registry.audit_module(&stored.entry.slot).unwrap();assert!(result.storage_integrity_checked && result.audit.static_profile_passed);assert_eq!(result.registry_scope,stored.registry.scope);assert_eq!(result.audit.reference,manifest.reference);no_authority(&result.audit);
        assert_eq!(before,snapshot(root.path()));assert_eq!(inventory,serde_json::to_value(registry.inspect().unwrap()).unwrap());
        for slot in ["../candidate-private-sentinel","latest","","A",&"f".repeat(64)]{assert!(registry.audit_module(slot).is_err());}
        let mut document=Document::from_bytes(include_bytes!("../examples/opaque-document-v2.json")).unwrap();if let Content::Capability{capability,..}=&mut document.layers[1].content{*capability=manifest.reference;}
        let resolved=registry.resolve_document(&document).unwrap();assert_eq!(resolved.bindings[0].state,"quarantined_no_runtime");assert!(!resolved.bindings[0].available);let output=private();assert!(document.export_png(&perveance::worker_io::PrivateDirectory::open(output.path()).unwrap(),"blocked.png").is_err());assert_eq!(fs::read_dir(output.path()).unwrap().count(),0);
        drop(registry);let reopened=Registry::open(root.path()).unwrap();assert!(reopened.audit_module(&stored.entry.slot).unwrap().audit.static_profile_passed);assert_eq!(before,snapshot(root.path()));
    }
    #[test]
    fn audit_rechecks_the_entire_inventory_scope_members_and_orphan_state(){
        let root=private();Registry::initialize(root.path()).unwrap();let bytes=Module::new().bytes();let m=manifest(&bytes);let mut registry=Registry::open(root.path()).unwrap();let first=registry.quarantine(&captured(&bytes,&m)).unwrap();
        let mut second=m.clone();second.reference.version="2.0.0".into();let second=registry.quarantine(&captured(&bytes,&second)).unwrap();
        let other_artifact=root.path().join(&second.entry.slot).join(registry::ARTIFACT_FILE);fs::write(&other_artifact,b"corruption in a different slot").unwrap();assert!(registry.audit_module(&first.entry.slot).is_err());fs::write(&other_artifact,&bytes).unwrap();
        let receipt=root.path().join(&first.entry.slot).join(registry::RECEIPT_FILE);let original=fs::read(&receipt).unwrap();let mut changed=original.clone();let position=changed.windows(32).position(|w|w==first.registry.scope.as_bytes()).unwrap();changed[position]=if changed[position]==b'a'{b'b'}else{b'a'};fs::write(&receipt,&changed).unwrap();assert!(registry.audit_module(&first.entry.slot).is_err());fs::write(&receipt,&original).unwrap();
        let extra=root.path().join(&first.entry.slot).join("unrecognized");fs::write(&extra,b"retained").unwrap();assert!(registry.audit_module(&first.entry.slot).is_err());fs::remove_file(extra).unwrap();
        let stage=root.path().join(format!(".perveance-publish-{}","0".repeat(32)));fs::create_dir(&stage).unwrap();fs::write(stage.join("partial"),b"retained uncertainty").unwrap();let before=snapshot(root.path());assert!(registry.audit_module(&first.entry.slot).is_err());assert!(registry.inspect().unwrap().mutations_blocked);assert_eq!(before,snapshot(root.path()));
    }
}

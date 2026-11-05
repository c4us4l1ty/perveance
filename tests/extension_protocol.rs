use perveance::{document::{self, CapabilityRef,Content,Document,OpaquePayload},extension_protocol::{self as wire,Error,Intent,EditIntent,Kind,PreparedCall},registry::{self,ArtifactDeclaration,ArtifactFormat,CapturedCandidate,EntryPoints,Manifest,OperationDeclaration,ProvenanceDeclaration,RuntimeDeclaration}};
use sha2::{Digest,Sha256};
fn id(n:u32)->String{format!("{n:032x}")}
fn uint(mut n:u32)->Vec<u8>{let mut b=Vec::new();loop{let d=(n&127)as u8;n>>=7;b.push(d|if n>0{128}else{0});if n==0{return b;}}}
fn text(s:&str)->Vec<u8>{let mut b=uint(s.len()as u32);b.extend(s.as_bytes());b}
fn module()->Vec<u8>{
    let mut exports=vec![5];for(name,kind,index)in[("memory",2,0),("perveance_alloc_v1",0,0),("validate_payload_v1",0,1),("apply_edits_v1",0,2),("render_layer_v1",0,3)]{exports.extend(text(name));exports.extend([kind,index]);}
    let mut bytes=b"\0asm\x01\0\0\0".to_vec();for(tag,section)in[(1,vec![2,0x60,2,0x7f,0x7f,1,0x7e,0x60,1,0x7f,1,0x7f]),(3,vec![4,1,0,0,0]),(5,vec![1,1,1,128,2]),(7,exports),(10,vec![4,4,0,0x41,0,0x0b,4,0,0x42,0,0x0b,4,0,0x42,0,0x0b,4,0,0x42,0,0x0b])]{bytes.push(tag);bytes.extend(uint(section.len()as u32));bytes.extend(section);}bytes
}
fn manifest(bytes:&[u8])->Manifest{Manifest{version:1,reference:CapabilityRef{id:"example.protocol".into(),version:"1.0.0".into(),artifact_sha256:format!("{:x}",Sha256::digest(bytes)),abi_version:1,payload_schema:1},artifact:ArtifactDeclaration{format:ArtifactFormat::WasmV1,bytes:bytes.len()as u64},document_version:2,
    entrypoints:EntryPoints{validate_payload:"validate_payload_v1".into(),apply_edits:"apply_edits_v1".into(),render_layer:"render_layer_v1".into()},operations:vec![OperationDeclaration{id:"example_edit".into(),schema_version:1}],runtime:RuntimeDeclaration{fuel:100000,memory_bytes:registry::MAX_MEMORY_BYTES,output_bytes:registry::MAX_OUTPUT_BYTES},provenance:ProvenanceDeclaration{source_sha256:"a".repeat(64),recipe_sha256:"b".repeat(64)}}}
fn candidate(m:&Manifest)->CapturedCandidate{CapturedCandidate::from_bytes(&m.canonical_bytes().unwrap(),&module()).unwrap()}
fn document(m:&Manifest)->Document{let mut doc=Document::from_bytes(include_bytes!("../examples/opaque-document-v2.json")).unwrap();if let Content::Capability{capability,..}=&mut doc.layers[1].content{*capability=m.reference.clone();}doc}
fn intent(doc:&Document,kind:Kind)->Intent{Intent{version:1,call_id:id(1),base_revision:doc.revision().unwrap(),layer_id:doc.layers[1].id.clone(),kind,edit:if kind==Kind::ApplyEdits{Some(EditIntent{operation:"example_edit".into(),schema_version:1,arguments:OpaquePayload::from_bytes(b"candidate-private-arguments").unwrap(),new_object_ids:vec![id(5),id(6)]})}else{None}}}
fn prepared(kind:Kind)->(PreparedCall,Document){let m=manifest(&module());let doc=document(&m);(candidate(&m).prepare_call(&doc,&intent(&doc,kind)).unwrap(),doc)}
fn unhex(s:&str)->Vec<u8>{(0..s.len()).step_by(2).map(|n|u8::from_str_radix(&s[n..n+2],16).unwrap()).collect()}
fn blob(bytes:&[u8])->Vec<u8>{let mut b=(bytes.len()as u32).to_le_bytes().to_vec();b.extend(bytes);b}
fn packet(call:&PreparedCall,status:u8,payload:&[u8])->Vec<u8>{let mut b=wire::REPLY_MAGIC.to_vec();let kind=match call.report().kind{Kind::ValidatePayload=>1,Kind::ApplyEdits=>2,Kind::RenderLayer=>3};b.extend([kind,status,0,0]);b.extend(unhex(&call.report().request_sha256));b.extend(blob(payload));b}
fn gates<T:serde::Serialize>(value:&T){assert!(serde_json::to_value(value).unwrap().as_object().unwrap().values().all(|v|v.as_bool()==Some(false)));}
fn check_bad(call:&PreparedCall,bytes:&[u8],error:Error){let r=call.inspect_reply(bytes);assert!(!r.wire_accepted);assert_eq!(r.issue,Some(error));assert!(r.projected_layer_sha256.is_none()&&r.rgba_sha256.is_none()&&r.candidate_status.is_none());gates(&r.gates);}
fn object_id(doc:&Document)->String{if let Content::Capability{objects,..}=&doc.layers[1].content{objects[0].id.clone()}else{panic!()}}
fn payload(doc:&Document)->Vec<u8>{if let Content::Capability{objects,..}=&doc.layers[1].content{objects[0].payload.bytes().unwrap()}else{panic!()}}
fn replace(doc:&Document,bytes:&[u8])->Vec<u8>{let mut b=vec![2];b.extend(unhex(&object_id(doc)));b.extend(Sha256::digest(payload(doc)));b.extend(blob(bytes));b}
fn remove(doc:&Document)->Vec<u8>{let mut b=vec![3];b.extend(unhex(&object_id(doc)));b.extend(Sha256::digest(payload(doc)));b}
fn insert(new_id:&str,before:Option<&str>,bytes:&[u8])->Vec<u8>{let mut b=vec![1];b.extend(unhex(new_id));match before{None=>b.push(0),Some(id)=>{b.push(1);b.extend(unhex(id));}}b.extend(blob(bytes));b}
fn changes(items:&[Vec<u8>])->Vec<u8>{let mut b=(items.len()as u32).to_le_bytes().to_vec();for item in items{b.extend(item);}b}
fn rgba(width:u32,height:u32,pixels:&[u8])->Vec<u8>{let mut b=width.to_le_bytes().to_vec();b.extend(height.to_le_bytes());b.extend([1,0,0,0]);b.extend(blob(pixels));b}

#[test]
fn all_three_requests_round_trip_and_hash_exact_canonical_frames_without_authority(){
    for kind in [Kind::ValidatePayload,Kind::ApplyEdits,Kind::RenderLayer]{let(call,doc)=prepared(kind);let r=call.report();let view=wire::decode_request(call.request_bytes()).unwrap();assert_eq!(view.kind,kind);assert_eq!(view.width,2);assert_eq!(view.height,1);assert_eq!(view.payload_schema,1);assert_eq!(view.objects.len(),1);assert_eq!(view.objects[0].payload.to_vec(),payload(&doc));assert_eq!(view.edit.is_some(),kind==Kind::ApplyEdits);
        let mut hash=Sha256::new();hash.update(b"perveance-extension-request-v1\0");hash.update(call.request_bytes());assert_eq!(r.request_sha256,format!("{:x}",hash.finalize()));gates(&r.gates);
        let native:&[u8]=b"private-native-name";assert!(!call.request_bytes().windows(native.len()).any(|w|w==native));let json=serde_json::to_string(&r).unwrap();assert!(!json.contains("private-arguments")&&!json.contains("opaque-private-sentinel"));
    }
}
#[test]
fn validation_success_is_a_candidate_claim_not_payload_semantic_verification(){let(call,doc)=prepared(Kind::ValidatePayload);let before=doc.canonical_bytes().unwrap();let r=call.inspect_reply(&packet(&call,0,&[]));assert!(r.wire_accepted);assert_eq!(r.candidate_status,Some(wire::CandidateStatus::Success));gates(&r.gates);assert_eq!(before,doc.canonical_bytes().unwrap());}
#[test]
fn object_edit_projection_is_ordered_bounded_and_never_applied_to_the_document(){
    let(call,doc)=prepared(Kind::ApplyEdits);let before=doc.canonical_bytes().unwrap();let body=changes(&[insert(&id(5),Some(&object_id(&doc)),b"new opaque payload"),replace(&doc,b"replacement opaque payload"),insert(&id(6),None,b"tail")]);
    let r=call.inspect_reply(&packet(&call,0,&body));assert!(r.wire_accepted);assert_eq!(r.proposed_changes,3);assert_eq!(r.projected_objects,Some(3));assert_eq!(r.projected_payload_bytes,Some(18+26+4));assert!(r.projected_layer_sha256.is_some());gates(&r.gates);assert_eq!(before,doc.canonical_bytes().unwrap());
    let r=call.inspect_reply(&packet(&call,0,&changes(&[remove(&doc)])));assert!(r.wire_accepted);assert_eq!(r.projected_objects,Some(0));
}
#[test]
fn rgba_shape_and_hash_are_checked_without_display_composition_or_png_export(){let(call,doc)=prepared(Kind::RenderLayer);let before=doc.canonical_bytes().unwrap();let pixels=[1,2,3,0,250,249,248,128];let r=call.inspect_reply(&packet(&call,0,&rgba(2,1,&pixels)));assert!(r.wire_accepted);assert_eq!(r.rgba_bytes,Some(8));assert_eq!(r.rgba_sha256,Some(format!("{:x}",Sha256::digest(pixels))));gates(&r.gates);assert_eq!(before,doc.canonical_bytes().unwrap());assert!(doc.render_raster().is_err());}
#[test]
fn fresh_nonce_arguments_id_pool_and_document_revision_each_change_request_binding(){
    let m=manifest(&module());let doc=document(&m);let c=candidate(&m);let i=intent(&doc,Kind::ApplyEdits);let base=c.prepare_call(&doc,&i).unwrap().report().request_sha256;
    let mut changed=i.clone();changed.call_id=id(8);assert_ne!(base,c.prepare_call(&doc,&changed).unwrap().report().request_sha256);
    changed=i.clone();changed.edit.as_mut().unwrap().arguments=OpaquePayload::from_bytes(b"other").unwrap();assert_ne!(base,c.prepare_call(&doc,&changed).unwrap().report().request_sha256);
    changed=i.clone();changed.edit.as_mut().unwrap().new_object_ids.reverse();assert_ne!(base,c.prepare_call(&doc,&changed).unwrap().report().request_sha256);
    let mut other=doc.clone();other.layers[0].visible=true;assert!(matches!(c.prepare_call(&other,&i),Err(Error::StaleRevision)));changed=i.clone();changed.base_revision=other.revision().unwrap();assert_ne!(base,c.prepare_call(&other,&changed).unwrap().report().request_sha256);
}
#[test]
fn exact_capability_pin_schema_and_supported_contract_are_required(){
    let m=manifest(&module());let c=candidate(&m);for field in 0..5{let mut doc=document(&m);if let Content::Capability{capability,..}=&mut doc.layers[1].content{match field{0=>capability.id="other".into(),1=>capability.version="1.0.1".into(),2=>capability.artifact_sha256="f".repeat(64),3=>capability.abi_version=2,_=>capability.payload_schema=2}}let i=intent(&doc,Kind::ValidatePayload);assert!(matches!(c.prepare_call(&doc,&i),Err(Error::ReferenceMismatch)));}
    let bytes=b"\0asm\x01\0\0\0";let mut bad=manifest(bytes);bad.artifact.bytes=8;let c=CapturedCandidate::from_bytes(&bad.canonical_bytes().unwrap(),bytes).unwrap();let doc=document(&bad);assert!(matches!(c.prepare_call(&doc,&intent(&doc,Kind::ValidatePayload)),Err(Error::StaticProfileRequired)));
}
#[test]
fn invalid_intents_operations_and_noncapability_scope_are_refused(){
    let m=manifest(&module());let doc=document(&m);let c=candidate(&m);let base=intent(&doc,Kind::ApplyEdits);
    let mut i=base.clone();i.version=2;assert!(matches!(c.prepare_call(&doc,&i),Err(Error::InvalidIntent)));
    i=base.clone();i.call_id="../candidate-private-sentinel".into();assert!(matches!(c.prepare_call(&doc,&i),Err(Error::InvalidIntent)));
    i=base.clone();i.edit=None;assert!(matches!(c.prepare_call(&doc,&i),Err(Error::InvalidIntent)));
    i=base.clone();i.kind=Kind::RenderLayer;assert!(matches!(c.prepare_call(&doc,&i),Err(Error::InvalidIntent)));
    for schema in [0,2]{i=base.clone();i.edit.as_mut().unwrap().schema_version=schema;assert!(matches!(c.prepare_call(&doc,&i),Err(Error::OperationNotDeclared)));}
    i=base.clone();i.edit.as_mut().unwrap().operation="undeclared".into();assert!(matches!(c.prepare_call(&doc,&i),Err(Error::OperationNotDeclared)));
    i=base;i.layer_id=doc.layers[0].id.clone();assert!(matches!(c.prepare_call(&doc,&i),Err(Error::ObjectScope)));
}
#[test]
fn proposed_new_ids_are_bounded_unique_and_cannot_collide_with_any_document_scope(){
    let m=manifest(&module());let doc=document(&m);let c=candidate(&m);for ids in [vec![id(5),id(5)],vec![doc.id.clone()],vec![doc.layers[0].id.clone()],vec![object_id(&doc)],(0..=wire::MAX_NEW_IDS).map(|n|id(n as u32+100)).collect()]{let mut i=intent(&doc,Kind::ApplyEdits);i.edit.as_mut().unwrap().new_object_ids=ids;assert!(matches!(c.prepare_call(&doc,&i),Err(Error::IdPool)));}
}
#[test]
fn intent_json_rejects_unknown_self_approval_duplicate_fields_and_oversize_arrays(){
    let(_,doc)=prepared(Kind::ApplyEdits);let i=intent(&doc,Kind::ApplyEdits);let mut value=serde_json::to_value(&i).unwrap();value["verified"]=serde_json::Value::Bool(true);assert!(Intent::from_bytes(&serde_json::to_vec(&value).unwrap()).is_err());
    value=serde_json::to_value(&i).unwrap();value["edit"]["new_object_ids"]=serde_json::json!((0..33).map(id).collect::<Vec<_>>());assert!(Intent::from_bytes(&serde_json::to_vec(&value).unwrap()).is_err());
    let mut bytes=serde_json::to_vec(&i).unwrap();bytes.pop();bytes.extend(b",\"version\":1}");assert!(Intent::from_bytes(&bytes).is_err());assert_eq!(Intent::from_bytes(&vec![b' ';wire::MAX_INTENT_BYTES as usize+1]).err(),Some(Error::FrameLimit));
}
#[test]
fn request_decoder_rejects_all_truncations_trailing_reserved_and_count_attacks(){
    let(call,_)=prepared(Kind::ApplyEdits);let bytes=call.request_bytes();for end in 0..bytes.len(){assert!(wire::decode_request(&bytes[..end]).is_err(),"prefix {end}");}
    let mut bad=bytes.to_vec();bad.push(0);assert!(wire::decode_request(&bad).is_err());bad=bytes.to_vec();bad[9]=1;assert!(wire::decode_request(&bad).is_err());bad=bytes.to_vec();bad[184..188].copy_from_slice(&u32::MAX.to_le_bytes());assert!(matches!(wire::decode_request(&bad),Err(Error::FrameLimit)));
    assert!(matches!(wire::decode_request(&vec![0;wire::MAX_REQUEST_BYTES+1]),Err(Error::FrameLimit)));
}
#[test]
fn reply_frame_rejects_truncation_trailing_flags_digest_kind_status_and_length_attacks(){
    let(call,_)=prepared(Kind::ValidatePayload);let bytes=packet(&call,0,&[]);assert_eq!(bytes.len(),wire::REPLY_HEADER_BYTES);for end in 0..bytes.len(){assert!(!call.inspect_reply(&bytes[..end]).wire_accepted);}
    let mut bad=bytes.clone();bad.push(0);check_bad(&call,&bad,Error::MalformedFrame);bad=bytes.clone();bad[10]=1;check_bad(&call,&bad,Error::MalformedFrame);
    bad=bytes.clone();bad[12]^=1;check_bad(&call,&bad,Error::RequestMismatch);bad=bytes.clone();bad[8]=2;check_bad(&call,&bad,Error::WrongKind);bad=bytes.clone();bad[9]=255;check_bad(&call,&bad,Error::UnknownStatus);bad=bytes;bad[44..48].copy_from_slice(&u32::MAX.to_le_bytes());check_bad(&call,&bad,Error::FrameLimit);
    check_bad(&call,&vec![0;wire::MAX_REPLY_BYTES as usize+1],Error::FrameLimit);
}
#[test]
fn candidate_failure_status_is_fixed_empty_and_never_a_successful_edit_or_render(){for kind in [Kind::ValidatePayload,Kind::ApplyEdits,Kind::RenderLayer]{let(call,_)=prepared(kind);for status in 1..=4{let r=call.inspect_reply(&packet(&call,status,&[]));assert!(r.wire_accepted);assert_ne!(r.candidate_status,Some(wire::CandidateStatus::Success));assert!(r.projected_layer_sha256.is_none()&&r.rgba_sha256.is_none());gates(&r.gates);check_bad(&call,&packet(&call,status,b"candidate-private-sentinel"),Error::PayloadShape);}}}
#[test]
fn validation_reply_cannot_smuggle_edit_render_or_private_error_payload(){let(call,_)=prepared(Kind::ValidatePayload);check_bad(&call,&packet(&call,0,b"candidate-private-sentinel"),Error::PayloadShape);let debug=format!("{:?}",call.inspect_reply(&packet(&call,0,b"candidate-private-sentinel")));assert!(!debug.contains("sentinel"));}
#[test]
fn wrong_old_hash_unknown_targets_unoffered_insertions_and_duplicate_targets_reject_atomically(){
    let(call,doc)=prepared(Kind::ApplyEdits);let before=doc.canonical_bytes().unwrap();let mut change=replace(&doc,b"x");change[17]^=1;check_bad(&call,&packet(&call,0,&changes(&[change])),Error::ObjectPrecondition);
    check_bad(&call,&packet(&call,0,&changes(&[insert(&id(99),None,b"x")])),Error::ObjectScope);
    check_bad(&call,&packet(&call,0,&changes(&[insert(&id(5),Some(&id(99)),b"x")])),Error::ObjectScope);
    check_bad(&call,&packet(&call,0,&changes(&[replace(&doc,b"x"),remove(&doc)])),Error::DuplicateTarget);
    check_bad(&call,&packet(&call,0,&changes(&[insert(&id(5),None,b"x"),insert(&id(5),None,b"y")])),Error::DuplicateTarget);
    let mut change=remove(&doc);change[1..17].copy_from_slice(&unhex(&doc.layers[0].id));check_bad(&call,&packet(&call,0,&changes(&[change])),Error::ObjectScope);assert_eq!(before,doc.canonical_bytes().unwrap());
}
#[test]
fn object_change_tags_count_payload_size_and_exact_end_are_bounded(){
    let(call,doc)=prepared(Kind::ApplyEdits);check_bad(&call,&packet(&call,0,&((wire::MAX_CHANGES+1)as u32).to_le_bytes()),Error::FrameLimit);
    let mut change=insert(&id(5),None,b"x");change[0]=255;check_bad(&call,&packet(&call,0,&changes(&[change])),Error::PayloadShape);
    check_bad(&call,&packet(&call,0,&changes(&[replace(&doc,&vec![0;document::MAX_PAYLOAD_BYTES+1])])),Error::FrameLimit);
    let mut body=changes(&[]);body.push(0);check_bad(&call,&packet(&call,0,&body),Error::MalformedFrame);
    let r=call.inspect_reply(&packet(&call,0,&changes(&[])));assert!(r.wire_accepted);assert_eq!(r.proposed_changes,0);
}
#[test]
fn global_object_and_payload_caps_include_unselected_layers(){
    let m=manifest(&module());let c=candidate(&m);let mut doc=document(&m);let mut extra=doc.layers[1].clone();extra.id=id(80);if let Content::Capability{objects,..}=&mut extra.content{objects.clear();for n in 0..document::MAX_OBJECTS-1{objects.push(document::CapabilityObject{id:id(n as u32+1000),payload:OpaquePayload::from_bytes(&[]).unwrap()});}}doc.layers.push(extra);doc.validate().unwrap();let call=c.prepare_call(&doc,&intent(&doc,Kind::ApplyEdits)).unwrap();check_bad(&call,&packet(&call,0,&changes(&[insert(&id(5),None,b"")])),Error::DocumentLimit);
    let mut doc=document(&m);if let Content::Capability{objects,..}=&mut doc.layers[1].content{objects[0].payload=OpaquePayload::from_bytes(&[]).unwrap();}let mut extra=doc.layers[1].clone();extra.id=id(80);if let Content::Capability{objects,..}=&mut extra.content{objects.clear();for n in 0..64{objects.push(document::CapabilityObject{id:id(n+1000),payload:OpaquePayload::from_bytes(&vec![1;document::MAX_PAYLOAD_BYTES]).unwrap()});}}doc.layers.push(extra);doc.validate().unwrap();let call=c.prepare_call(&doc,&intent(&doc,Kind::ApplyEdits)).unwrap();check_bad(&call,&packet(&call,0,&changes(&[insert(&id(5),None,b"x")])),Error::DocumentLimit);
}
#[test]
fn rgba_dimensions_format_lengths_reserved_and_trailing_bytes_are_exact(){let(call,_)=prepared(Kind::RenderLayer);for body in [rgba(1,2,&[0;8]),rgba(u32::MAX,1,&[0;8]),rgba(2,1,&[0;7])]{assert!(!call.inspect_reply(&packet(&call,0,&body)).wire_accepted);}let mut body=rgba(2,1,&[0;8]);body[8]=2;check_bad(&call,&packet(&call,0,&body),Error::RenderShape);body=rgba(2,1,&[0;8]);body[9]=1;check_bad(&call,&packet(&call,0,&body),Error::MalformedFrame);body=rgba(2,1,&[0;8]);body.push(0);check_bad(&call,&packet(&call,0,&body),Error::MalformedFrame);}
#[test]
fn packed_memory_ranges_use_unsigned_fields_checked_arithmetic_and_declared_limits(){
    let(call,_)=prepared(Kind::ValidatePayload);let bytes=packet(&call,0,&[]);let mut memory=vec![0;65536];let pointer=65536-bytes.len();memory[pointer..].copy_from_slice(&bytes);let packed=(((pointer as u64)<<32)|bytes.len()as u64)as i64;let r=call.inspect_memory_reply(&memory,packed);assert!(r.wire_accepted&&r.memory_window_checked);gates(&r.gates);
    for packed in [0,-1,((u32::MAX as u64)<<32|48)as i64,((65535u64)<<32|48)as i64,((1u64)<<32)as i64]{assert!(!call.inspect_memory_reply(&memory,packed).wire_accepted);}
    assert_eq!(call.request_window(65536,1).unwrap(),1..1+call.request_bytes().len());for (size,ptr)in [(0,1),(65535,1),(65536,0),(65536,u32::MAX),(registry::MAX_MEMORY_BYTES as usize+65536,1)]{assert_eq!(call.request_window(size,ptr).err(),Some(Error::MemoryWindow));}
}
#[test]
fn manifest_output_and_input_memory_declarations_bound_data_checks_not_execution(){let mut m=manifest(&module());m.runtime.output_bytes=48;let doc=document(&m);let call=candidate(&m).prepare_call(&doc,&intent(&doc,Kind::ApplyEdits)).unwrap();check_bad(&call,&packet(&call,0,&changes(&[])),Error::FrameLimit);m.runtime.output_bytes=47;let doc=document(&m);assert!(matches!(candidate(&m).prepare_call(&doc,&intent(&doc,Kind::ValidatePayload)),Err(Error::FrameLimit)));}
#[test]
fn oversized_scoped_request_cannot_fit_a_small_declared_memory_budget(){
    let mut bytes=module();let marker=[5,5,1,1,1,128,2];let at=bytes.windows(marker.len()).position(|w|w==marker).unwrap();bytes.splice(at..at+marker.len(),[5,4,1,1,1,1]);
    let mut m=manifest(&bytes);m.runtime.memory_bytes=65536;let c=CapturedCandidate::from_bytes(&m.canonical_bytes().unwrap(),&bytes).unwrap();let mut doc=document(&m);
    if let Content::Capability{objects,..}=&mut doc.layers[1].content{objects.clear();for n in 0..5{objects.push(document::CapabilityObject{id:id(n+100),payload:OpaquePayload::from_bytes(&vec![0;document::MAX_PAYLOAD_BYTES]).unwrap()});}}
    assert!(matches!(c.prepare_call(&doc,&intent(&doc,Kind::ValidatePayload)),Err(Error::FrameLimit)));
}
#[test]
fn mutation_corpus_cannot_gain_host_authority_or_mutate_the_captured_document(){let(call,doc)=prepared(Kind::ApplyEdits);let before=doc.canonical_bytes().unwrap();let bytes=packet(&call,0,&changes(&[replace(&doc,b"x")]));for n in 0..bytes.len(){let mut mutation=bytes.clone();mutation[n]^=0x80;let r=call.inspect_reply(&mutation);gates(&r.gates);if !r.wire_accepted{assert!(r.projected_layer_sha256.is_none());}}assert_eq!(before,doc.canonical_bytes().unwrap());}

#[cfg(target_os="linux")]
mod linux {
    use super::*;use perveance::registry::Registry;use std::{fs,os::unix::fs::PermissionsExt};
    fn private()->tempfile::TempDir{tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o700)).tempdir().unwrap()}
    #[test]
    fn registry_protocol_inspection_is_read_only_exact_scoped_and_still_unavailable(){let root=private();Registry::initialize(root.path()).unwrap();let m=manifest(&module());let doc=document(&m);let i=intent(&doc,Kind::ValidatePayload);let mut registry=Registry::open(root.path()).unwrap();let admitted=registry.quarantine(&candidate(&m)).unwrap();let receipt=root.path().join(&admitted.entry.slot).join(registry::RECEIPT_FILE);let before=fs::read(&receipt).unwrap();let call=candidate(&m).prepare_call(&doc,&i).unwrap();let prepared=registry.inspect_protocol(&admitted.entry.slot,&doc,&i,None).unwrap();let mut response=packet(&call,0,&[]);response[12..44].copy_from_slice(&unhex(&prepared.call.request_sha256));let r=registry.inspect_protocol(&admitted.entry.slot,&doc,&i,Some(&response)).unwrap();assert!(r.storage_integrity_checked&&r.reply.unwrap().wire_accepted);gates(&r.call.gates);assert_eq!(before,fs::read(&receipt).unwrap());assert_eq!(registry.resolve_document(&doc).unwrap().bindings[0].state,"quarantined_no_runtime");assert_eq!(fs::read_dir(root.path().join(&admitted.entry.slot)).unwrap().count(),3);let stage=root.path().join(format!(".perveance-publish-{}",id(90)));fs::create_dir(&stage).unwrap();assert!(registry.inspect_protocol(&admitted.entry.slot,&doc,&i,None).is_err());assert!(stage.exists());}
}

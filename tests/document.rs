use perveance::{document::{self, CapabilityObject, CapabilityRef, Content, Document, DocumentLayer, OpaquePayload, Operation, PixelEdit, Proposal}, Project};
use serde_json::json;

fn png(project: &Project) -> Vec<u8> {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("reference.png");
    project.export_png_new(&path).unwrap();
    std::fs::read(path).unwrap()
}

fn raster() -> Project {
    let mut project = Project::new(2, 2).unwrap();
    project.layers[0].name = "private-layer-sentinel".into();
    project.layers[0].visible = false;
    project.set_pixel(0, 1, 1, [1, 2, 3, 0]).unwrap();
    project.add_layer().unwrap();
    project.set_pixel(1, 0, 0, [9, 8, 7, 128]).unwrap();
    project
}
fn opaque() -> Document {
    let mut doc = Document::from_project(&raster()).unwrap();
    doc.layers.push(DocumentLayer { id: format!("{:032x}", 500), name: "unavailable-private-sentinel".into(), visible: false,
        content: Content::Capability { capability: CapabilityRef { id: "example.unavailable".into(), version: "1.2.3".into(),
            artifact_sha256: "f".repeat(64), abi_version: 99, payload_schema: 77 },
            objects: vec![CapabilityObject { id: format!("{:032x}", 501), payload: OpaquePayload::from_bytes(&payload()).unwrap() }] } });
    doc.validate().unwrap(); doc
}
fn payload() -> Vec<u8> {
    let mut bytes: Vec<u8> = (0..=255).collect();
    bytes.extend_from_slice(b"\0{ not JSON }\n#!/bin/sh\nprivate-payload-sentinel\xff\r\n"); bytes
}
fn proposal(doc: &Document, operations: Vec<Operation>) -> Proposal {
    Proposal { version: 1, base_revision: doc.revision().unwrap(), operations }
}

#[test]
fn native_import_is_lossless_deterministic_and_uses_the_existing_engine() {
    let source = raster();
    let doc = Document::from_project(&source).unwrap();
    assert_eq!(doc, Document::from_project(&source).unwrap());
    assert_eq!(doc.render_raster().unwrap(), source);
    assert_eq!(png(&doc.render_raster().unwrap()), png(&source));
    assert_eq!(Document::from_bytes(&doc.canonical_bytes().unwrap()).unwrap(), doc);
    assert_eq!(doc.inspect().unwrap().rendering, "native_raster");
    assert_ne!(doc.revision().unwrap(), source.revision().unwrap());
}

#[test]
fn opaque_binary_bytes_and_future_references_round_trip_without_parsing_or_loading() {
    let doc = opaque();
    let copy = Document::from_bytes(&doc.canonical_bytes().unwrap()).unwrap();
    assert_eq!(copy, doc);
    let Content::Capability { capability, objects } = &copy.layers[2].content else { panic!("missing opaque content"); };
    assert_eq!(capability.abi_version, 99); assert_eq!(capability.payload_schema, 77);
    assert_eq!(objects[0].payload.bytes().unwrap(), payload());
    let report = copy.inspect().unwrap();
    assert_eq!(report.unavailable_capabilities.len(), 1);
    assert_eq!(report.rendering, "blocked_unavailable_capabilities");
    assert!(!report.code_loaded && !report.registry_consulted && !report.opaque_payloads_validated && !report.autonomous_execution);
    let text = serde_json::to_string(&report).unwrap();
    for secret in ["private-layer-sentinel", "unavailable-private-sentinel", "private-payload-sentinel", &objects[0].payload.data] {
        assert!(!text.contains(secret));
    }
    assert!(!format!("{doc:?}").contains("sentinel"));
    assert!(!format!("{:?}", objects[0].payload).contains("sentinel"));
}

#[test]
fn even_hidden_or_empty_unavailable_layers_block_all_rendering() {
    let mut doc = opaque();
    assert!(!doc.layers[2].visible);
    assert!(doc.render_raster().is_err());
    if let Content::Capability { objects, .. } = &mut doc.layers[2].content { objects.clear(); }
    assert!(doc.render_raster().is_err());
    doc.layers[2].visible = true;
    assert!(doc.render_raster().is_err());
}

#[test]
fn stable_id_edits_preserve_every_unavailable_byte_and_transparent_raster_byte() {
    let mut doc = opaque();
    let original = doc.clone();
    let id = doc.layers[0].id.clone();
    let opaque_id = doc.layers[2].id.clone();
    let edit = proposal(&doc, vec![
        Operation::MoveLayer { layer_id: id.clone(), before: None },
        Operation::RenameLayer { layer_id: opaque_id.clone(), name: "renamed wrapper only".into() },
        Operation::SetVisibility { layer_id: opaque_id.clone(), visible: true },
        Operation::RasterPixels { layer_id: id.clone(), edits: vec![PixelEdit { x: 0, y: 1, color: [12, 34, 56, 0] }] },
    ]);
    doc.apply(&edit).unwrap();
    assert_eq!(doc.id, original.id); assert_eq!(doc.layers[2].id, id);
    let layer = doc.layers.iter().find(|layer| layer.id == opaque_id).unwrap();
    assert!(layer.content == original.layers[2].content);
    assert!(layer.visible); assert_eq!(layer.name, "renamed wrapper only");
    let Content::Raster { pixels } = &doc.layers[2].content else { panic!("not raster"); };
    assert_eq!(pixels[2], [12, 34, 56, 0]); assert_eq!(pixels[3], [1, 2, 3, 0]);
    assert_eq!(Document::from_bytes(&doc.canonical_bytes().unwrap()).unwrap(), doc);
    assert!(doc.render_raster().is_err());
    let before_stale = doc.clone(); assert!(doc.apply(&edit).is_err()); assert_eq!(doc, before_stale);
}

#[test]
fn native_edits_reordering_and_composition_match_v1_by_identity_not_old_index() {
    let mut native = raster(); let mut doc = Document::from_project(&native).unwrap();
    let first = doc.layers[0].id.clone(); let second = doc.layers[1].id.clone();
    doc.apply(&proposal(&doc, vec![
        Operation::MoveLayer { layer_id: second, before: Some(first.clone()) },
        Operation::SetVisibility { layer_id: first.clone(), visible: true },
        Operation::RasterPixels { layer_id: first, edits: vec![PixelEdit { x: 0, y: 0, color: [91, 92, 93, 127] }] },
    ])).unwrap();
    native.move_layer(1, 0).unwrap(); native.layers[1].visible = true;
    native.set_pixel(1, 0, 0, [91, 92, 93, 127]).unwrap();
    assert_eq!(doc.render_raster().unwrap(), native);
    assert_eq!(png(&doc.render_raster().unwrap()), png(&native));
}

#[test]
fn failed_batches_cannot_partially_rename_reorder_or_edit_opaque_objects() {
    let original = opaque();
    let valid = Operation::RenameLayer { layer_id: original.layers[0].id.clone(), name: "would mutate".into() };
    let failures = vec![
        Operation::RasterPixels { layer_id: original.layers[2].id.clone(), edits: vec![PixelEdit { x: 0, y: 0, color: [1;4] }] },
        Operation::RasterPixels { layer_id: original.layers[0].id.clone(), edits: vec![PixelEdit { x: u32::MAX, y: 0, color: [1;4] }] },
        Operation::MoveLayer { layer_id: original.layers[0].id.clone(), before: Some("a".repeat(32)) },
        Operation::RenameLayer { layer_id: original.layers[1].id.clone(), name: " ".into() },
        Operation::SetVisibility { layer_id: original.id.clone(), visible: true },
    ];
    for invalid in failures {
        let mut doc = original.clone(); assert!(doc.apply(&proposal(&original, vec![valid.clone(), invalid])).is_err()); assert_eq!(doc, original);
    }
}

#[test]
fn root_layer_and_object_ids_are_strict_and_unique_in_one_namespace() {
    let original = opaque();
    for bad in ["", "A0000000000000000000000000000000", "../unsafe", &"0".repeat(31), &"0".repeat(33)] {
        let mut doc = original.clone(); doc.layers[0].id = bad.into(); assert!(doc.validate().is_err());
    }
    let mut doc = original.clone(); doc.layers[0].id = doc.id.clone(); assert!(doc.validate().is_err());
    let mut doc = original.clone(); doc.layers[0].id = doc.layers[1].id.clone(); assert!(doc.validate().is_err());
    let mut doc = original.clone(); let duplicate = doc.layers[0].id.clone();
    if let Content::Capability { objects, .. } = &mut doc.layers[2].content { objects[0].id = duplicate; }
    assert!(doc.validate().is_err());
}

#[test]
fn capability_references_are_exact_bounded_metadata_not_instructions() {
    let doc = opaque();
    let Content::Capability { capability, .. } = &doc.layers[2].content else { panic!("not capability"); };
    for id in ["", "../plugin", "https://host/module", "/tmp/exec", "a b", &"x".repeat(65)] {
        let mut cap = capability.clone(); cap.id = id.into(); assert!(cap.validate().is_err());
    }
    for version in ["latest", "^1.2.3", "1.2", "01.2.3", "1.2.3+branch", "4294967296.0.0", "1.2.3\n"] {
        let mut cap = capability.clone(); cap.version = version.into(); assert!(cap.validate().is_err());
    }
    let mut cap = capability.clone(); cap.artifact_sha256 = "F".repeat(64); assert!(cap.validate().is_err());
    cap = capability.clone(); cap.abi_version = 0; assert!(cap.validate().is_err());
    cap = capability.clone(); cap.payload_schema = 0; assert!(cap.validate().is_err());
}

#[test]
fn opaque_payload_size_encoding_and_aggregate_limits_are_enforced() {
    let exact = OpaquePayload::from_bytes(&vec![0xff; document::MAX_PAYLOAD_BYTES]).unwrap();
    assert_eq!(exact.bytes().unwrap().len(), document::MAX_PAYLOAD_BYTES);
    assert!(OpaquePayload::from_bytes(&vec![0; document::MAX_PAYLOAD_BYTES+1]).is_err());
    for text in ["0", "AA", "zz", "00\n", "é"] {
        let mut invalid = exact.clone(); invalid.data = text.into(); assert!(invalid.bytes().is_err());
    }
    let mut doc = opaque();
    if let Content::Capability { objects, .. } = &mut doc.layers[2].content {
        *objects = (0..64).map(|i| CapabilityObject { id: format!("{:032x}", 1000+i), payload: exact.clone() }).collect();
    }
    doc.validate().unwrap();
    if let Content::Capability { objects, .. } = &mut doc.layers[2].content {
        objects.push(CapabilityObject { id: format!("{:032x}", 2000), payload: OpaquePayload::from_bytes(&[0]).unwrap() });
    }
    assert!(doc.validate().is_err());
    let mut doc = opaque();
    if let Content::Capability { objects, .. } = &mut doc.layers[2].content {
        *objects = (0..=document::MAX_OBJECTS).map(|i| CapabilityObject { id: format!("{:032x}", 1000+i), payload: OpaquePayload::from_bytes(&[]).unwrap() }).collect();
    }
    assert!(doc.validate().is_err());
    assert!(Document::from_bytes(&serde_json::to_vec(&doc).unwrap()).is_err());
}

#[test]
fn malformed_unknown_duplicate_or_legacy_fields_never_become_lossy_v2_documents() {
    let original = opaque(); let value = serde_json::to_value(&original).unwrap();
    let mut cases = vec![];
    let mut v=value.clone(); v["command"] = json!("private-payload-sentinel"); cases.push(v);
    let mut v=value.clone(); v["version"] = json!(3); cases.push(v);
    let mut v=value.clone(); v["width"] = json!(u32::MAX); cases.push(v);
    let mut v=value.clone(); v["layers"][0]["content"]["kind"] = json!("future_renderer"); cases.push(v);
    let mut v=value.clone(); v["layers"][2]["content"]["capability"]["url"] = json!("private-payload-sentinel"); cases.push(v);
    let mut v=value.clone(); v["layers"][2]["content"]["objects"][0]["payload"]["encoding"] = json!("base64"); cases.push(v);
    let mut v=value.clone(); v["layers"][0]["content"]["pixels"] = json!([[0,0,0,0]]); cases.push(v);
    for invalid in cases {
        let error = Document::from_bytes(&serde_json::to_vec(&invalid).unwrap()).unwrap_err(); assert!(!error.contains("sentinel"));
    }
    let mut duplicate = original.canonical_bytes().unwrap(); duplicate.pop(); duplicate.extend_from_slice(br#", "version":2}"#);
    assert!(Document::from_bytes(&duplicate).is_err());
    assert!(Document::from_bytes(&serde_json::to_vec(&raster()).unwrap()).is_err());
    assert!(serde_json::from_slice::<Project>(&original.canonical_bytes().unwrap()).is_err());
    let mut excessive = original.clone();
    excessive.layers = (0..=document::MAX_LAYERS).map(|i| { let mut layer=original.layers[0].clone(); layer.id=format!("{:032x}", 3000+i); layer }).collect();
    assert!(Document::from_bytes(&serde_json::to_vec(&excessive).unwrap()).is_err());
}

#[test]
fn native_sequence_limit_is_enforced_before_buffering_the_rest_of_tagged_content() {
    // Put the tag LAST and invalid JSON even later. A serde internally-tagged
    // Content buffer would first scan/allocate the entire input and report that
    // later syntax error. Our direct typed visitor stops at the first bound.
    let pixels = vec!["[0,0,0,0]"; (perveance::MAX_DIMENSION * perveance::MAX_DIMENSION) as usize + 1].join(",");
    let bytes = format!(r#"{{"pixels":[{pixels}],"kind":"raster","later":not_json}}"#);
    let error = serde_json::from_str::<Content>(&bytes).err().unwrap().to_string();
    assert!(error.contains("sequence exceeds limit"), "{error}");
    let object = r#"{"id":"00000000000000000000000000000001","payload":{"encoding":"hex","data":""}}"#;
    let objects = vec![object; document::MAX_OBJECTS + 1].join(",");
    let bytes = format!(r#"{{"objects":[{objects}],"kind":"capability","later":not_json}}"#);
    let error = serde_json::from_str::<Content>(&bytes).err().unwrap().to_string();
    assert!(error.contains("sequence exceeds limit"), "{error}");
}

#[test]
fn document_proposals_have_distinct_revision_format_and_bounded_edit_units() {
    let original = opaque(); let layer_id = original.layers[0].id.clone();
    let rename = Operation::RenameLayer { layer_id: layer_id.clone(), name: "new".into() };
    let mut edit = proposal(&original, vec![rename.clone()]); edit.version = 2;
    let mut doc = original.clone(); assert!(doc.apply(&edit).is_err());
    edit.version = 1; edit.base_revision = raster().revision().unwrap(); assert!(doc.apply(&edit).is_err());
    edit.base_revision = original.revision().unwrap(); edit.operations.clear(); assert!(doc.apply(&edit).is_err());
    edit.operations = vec![rename; document::MAX_EDIT_UNITS+1]; assert!(doc.apply(&edit).is_err());
    assert!(Proposal::from_bytes(&serde_json::to_vec(&edit).unwrap()).is_err());
    edit.operations = vec![Operation::RasterPixels { layer_id: layer_id.clone(), edits: vec![PixelEdit { x: 0, y: 0, color: [1;4] }; 600] },
        Operation::RasterPixels { layer_id, edits: vec![PixelEdit { x: 0, y: 0, color: [2;4] }; 425] }];
    assert!(doc.apply(&edit).is_err()); assert_eq!(doc, original);
    assert!(Proposal::from_bytes(&vec![b' '; document::MAX_PROPOSAL_BYTES as usize+1]).is_err());
    let arbitrary = json!({"version":1,"base_revision":original.revision().unwrap(),"operations":[{"op":"set_object_payload","id":"private-payload-sentinel","payload":"run"}]});
    assert!(Proposal::from_bytes(&serde_json::to_vec(&arbitrary).unwrap()).is_err());
}

#[cfg(target_os = "linux")]
mod private_io {
    use super::*;
    use perveance::worker_io::PrivateDirectory;
    use std::{fs, os::unix::fs::{PermissionsExt, symlink}};
    fn root() -> tempfile::TempDir { tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o700)).tempdir().unwrap() }
    #[test]
    fn document_publication_is_atomic_private_no_replace_and_bound_to_held_directory() {
        let dir = root(); let held = PrivateDirectory::open(dir.path()).unwrap(); let doc = opaque();
        let result = doc.publish(&held, "document.json").unwrap();
        assert!(result.committed && result.parent_directory_synced && !result.autonomous_execution);
        let path = dir.path().join("document.json"); assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(Document::capture(&held, "document.json").unwrap(), doc);
        assert!(doc.publish(&held, "document.json").is_err()); assert_eq!(Document::capture(&held, "document.json").unwrap(), doc);
        symlink("document.json", dir.path().join("linked.json")).unwrap();
        assert!(Document::capture(&held, "linked.json").is_err());
        assert!(doc.publish(&held, "linked.json").is_err());
        for name in ["../escape", ".hidden", "-option", "_name", "sub/file", "a\\b", ""] { assert!(doc.publish(&held, name).is_err()); }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        let parent=root(); let moved=parent.path().join("held"); fs::rename(dir.path(), &moved).unwrap();
        fs::create_dir(dir.path()).unwrap(); fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        doc.publish(&held, "after-rename.json").unwrap(); assert!(moved.join("after-rename.json").exists());
        assert!(!dir.path().join("after-rename.json").exists());
    }
    #[test]
    fn unsupported_export_creates_no_file_or_stage_and_native_export_matches_v1_bytes() {
        let dir=root(); let held=PrivateDirectory::open(dir.path()).unwrap();
        assert!(opaque().export_png(&held, "blocked.png").is_err()); assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
        let source=raster(); let doc=Document::from_project(&source).unwrap(); doc.export_png(&held, "native.png").unwrap();
        assert_eq!(fs::read(dir.path().join("native.png")).unwrap(), png(&source));
        assert!(doc.export_png(&held, "native.png").is_err()); assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}

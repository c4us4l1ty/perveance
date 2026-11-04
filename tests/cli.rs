use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);
struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("perveance-test-{}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_perveance-cli")).args(args).output().unwrap()
}

#[test]
fn headless_pixel_round_trip_and_overwrite_refusal() {
    let dir = TempDir::new();
    let initial = dir.0.join("initial.json");
    let painted = dir.0.join("painted.json");
    let png = dir.0.join("sprite.png");
    let initial = initial.to_str().unwrap();
    let painted = painted.to_str().unwrap();
    let png = png.to_str().unwrap();
    assert!(cli(&["new", "8", "8", initial]).status.success());
    assert!(cli(&["paint", initial, painted, "0", "2", "3", "11", "22", "33", "255"]).status.success());
    let before = fs::read(painted).unwrap();
    assert!(!cli(&["paint", initial, painted, "0", "1", "1", "1", "2", "3", "255"]).status.success());
    assert_eq!(before, fs::read(painted).unwrap());
    let inspection = cli(&["inspect", painted]);
    let value: serde_json::Value = serde_json::from_slice(&inspection.stdout).unwrap();
    assert_eq!(value["width"], 8);
    assert!(cli(&["export-png", painted, png]).status.success());
    let mut reader = png::Decoder::new(fs::File::open(png).unwrap()).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!((info.width, info.height), (8, 8));
    let offset = (3 * 8 + 2) * 4;
    assert_eq!(&pixels[offset..offset + 4], &[11, 22, 33, 255]);
}

#[test]
fn malformed_projects_fail_without_writing_an_output() {
    let dir = TempDir::new();
    let input = dir.0.join("bad.json");
    let output = dir.0.join("out.png");
    fs::write(&input, b"not a project").unwrap();
    let result = cli(&["export-png", input.to_str().unwrap(), output.to_str().unwrap()]);
    assert!(!result.status.success());
    assert!(!output.exists());
    let error: serde_json::Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(error["ok"], false);
}

#[test]
fn batch_can_create_layers_and_failing_batch_leaves_no_output() {
    let dir = TempDir::new();
    let initial = dir.0.join("initial.json");
    let output = dir.0.join("out.json");
    let rejected = dir.0.join("rejected.json");
    let operations = dir.0.join("operations.json");
    assert!(cli(&["new", "4", "4", initial.to_str().unwrap()]).status.success());
    fs::write(&operations, r#"[
        {"op":"add_layer"},
        {"op":"fill","layer":1,"x":0,"y":0,"color":[12,34,56,255]},
        {"op":"line","layer":1,"from":[0,0],"to":[3,3],"color":[255,0,0,255]}
    ]"#).unwrap();
    let result = cli(&["apply", initial.to_str().unwrap(), output.to_str().unwrap(), operations.to_str().unwrap()]);
    assert!(result.status.success());
    let parsed: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(parsed["applied"], 3);
    let project = perveance::Project::load(&output).unwrap();
    assert_eq!(project.layers.len(), 2);
    assert_eq!(project.layers[1].pixels[15], [255, 0, 0, 255]);
    assert_eq!(project.layers[1].pixels[1], [12, 34, 56, 255]);

    fs::write(&operations, r#"[{"op":"add_layer"},{"op":"pixel","layer":50,"x":0,"y":0,"color":[0,0,0,0]}]"#).unwrap();
    let result = cli(&["apply", initial.to_str().unwrap(), rejected.to_str().unwrap(), operations.to_str().unwrap()]);
    assert!(!result.status.success());
    assert!(!rejected.exists());
    assert_eq!(perveance::Project::load(&initial).unwrap().layers.len(), 1);
}

#[test]
fn batch_layer_controls_and_rectangle_export() {
    let dir = TempDir::new();
    let initial = dir.0.join("initial.json");
    let output = dir.0.join("out.json");
    let png = dir.0.join("out.png");
    let operations = dir.0.join("operations.json");
    assert!(cli(&["new", "4", "3", initial.to_str().unwrap()]).status.success());
    fs::write(&operations, r#"[
        {"op":"add_layer"},
        {"op":"rename_layer","layer":1,"name":"Ink"},
        {"op":"rectangle","layer":1,"x":1,"y":1,"width":2,"height":2,"color":[200,100,50,255]},
        {"op":"move_layer","from":1,"to":0},
        {"op":"remove_layer","layer":1}
    ]"#).unwrap();
    let result = cli(&["apply", initial.to_str().unwrap(), output.to_str().unwrap(), operations.to_str().unwrap()]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let project = perveance::Project::load(&output).unwrap();
    assert_eq!(project.layers.len(), 1);
    assert_eq!(project.layers[0].name, "Ink");
    assert_eq!(project.layers[0].pixels[5], [200, 100, 50, 255]);
    assert!(cli(&["export-png", output.to_str().unwrap(), png.to_str().unwrap()]).status.success());
    let mut reader = png::Decoder::new(fs::File::open(png).unwrap()).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut pixels).unwrap();
    assert_eq!(&pixels[20..24], &[200, 100, 50, 255]);
}

#[test]
fn discovery_reports_batch_support_and_version() {
    let output = cli(&["capabilities"]);
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["api_version"], 1);
    assert_eq!(value["autonomous_execution"], false);
    assert!(value["batch_operations"].as_array().unwrap().iter().any(|v| v == "rectangle"));
}

#[test]
fn example_potion_is_layered_and_transparent() {
    let ops: Vec<perveance::Operation> = serde_json::from_str(include_str!("../examples/potion-operations.json")).unwrap();
    let mut project = perveance::Project::new(16, 16).unwrap();
    project.apply_operations(&ops).unwrap();
    assert_eq!(project.layers.iter().map(|layer| layer.name.as_str()).collect::<Vec<_>>(), ["Background", "Bottle", "Highlights"]);
    let rgba = project.rgba().unwrap();
    assert_eq!(&rgba[0..4], &[0, 0, 0, 0]);
    let pixel = (7 * 16 + 6) * 4;
    assert_eq!(&rgba[pixel..pixel + 4], &[239, 204, 245, 255]);
}

#[test]
fn raster_shapes_are_discoverable_and_batch_errors_are_atomic() {
    let dir = TempDir::new();
    let initial = dir.0.join("base.json");
    let output = dir.0.join("shapes.json");
    let bad_output = dir.0.join("bad.json");
    let operations = dir.0.join("shapes-ops.json");
    assert!(cli(&["new", "7", "7", initial.to_str().unwrap()]).status.success());
    fs::write(&operations, r#"[
        {"op":"rectangle_outline","layer":0,"x":1,"y":1,"width":5,"height":5,"color":[255,0,0,255]},
        {"op":"ellipse","layer":0,"x":2,"y":2,"width":3,"height":3,"filled":true,"color":[0,0,255,255]}
    ]"#).unwrap();
    let result = cli(&["apply", initial.to_str().unwrap(), output.to_str().unwrap(), operations.to_str().unwrap()]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let project = perveance::Project::load(&output).unwrap();
    assert_eq!(project.layers[0].pixels[8], [255, 0, 0, 255]);
    assert_eq!(project.layers[0].pixels[24], [0, 0, 255, 255]);
    fs::write(&operations, r#"[
        {"op":"rectangle_outline","layer":0,"x":0,"y":0,"width":2,"height":2,"color":[255,0,0,255]},
        {"op":"ellipse","layer":0,"x":6,"y":6,"width":2,"height":2,"filled":false,"color":[0,0,255,255]}
    ]"#).unwrap();
    assert!(!cli(&["apply", initial.to_str().unwrap(), bad_output.to_str().unwrap(), operations.to_str().unwrap()]).status.success());
    assert!(!bad_output.exists());
    assert_eq!(perveance::Project::load(&initial).unwrap().layers[0].pixels[0], [0; 4]);
    let value: serde_json::Value = serde_json::from_slice(&cli(&["capabilities"]).stdout).unwrap();
    assert!(value["batch_operations"].as_array().unwrap().iter().any(|v| v == "ellipse"));
}

#[test]
fn sample_reports_raw_layer_or_visible_composite_without_mutation() {
    let dir = TempDir::new();
    let initial = dir.0.join("initial.json");
    let edited = dir.0.join("edited.json");
    let operations = dir.0.join("operations.json");
    assert!(cli(&["new", "2", "2", initial.to_str().unwrap()]).status.success());
    fs::write(&operations, r#"[
        {"op":"pixel","layer":0,"x":0,"y":0,"color":[20,40,60,200]},
        {"op":"add_layer"},
        {"op":"pixel","layer":1,"x":0,"y":0,"color":[200,120,50,100]},
        {"op":"set_visibility","layer":1,"visible":false}
    ]"#).unwrap();
    assert!(cli(&["apply", initial.to_str().unwrap(), edited.to_str().unwrap(), operations.to_str().unwrap()]).status.success());
    let path = edited.to_str().unwrap();
    let before = fs::read(&edited).unwrap();
    let raw = cli(&["sample", path, "0", "0", "layer", "1"]);
    assert!(raw.status.success());
    let raw: serde_json::Value = serde_json::from_slice(&raw.stdout).unwrap();
    assert_eq!(raw["color"], serde_json::json!([200, 120, 50, 100]));
    let composite = cli(&["sample", path, "0", "0", "composite"]);
    assert!(composite.status.success());
    let composite: serde_json::Value = serde_json::from_slice(&composite.stdout).unwrap();
    assert_eq!(composite["color"], serde_json::json!([20, 40, 60, 200]));
    for args in [vec!["sample", path, "2", "0", "composite"], vec!["sample", path, "0", "0", "layer", "2"]] {
        let result = cli(&args);
        assert!(!result.status.success());
        let error: serde_json::Value = serde_json::from_slice(&result.stderr).unwrap();
        assert_eq!(error["ok"], false);
    }
    assert_eq!(fs::read(&edited).unwrap(), before);
}

#[test]
fn batch_layer_transforms_reopen_and_fail_atomically() {
    let dir = TempDir::new();
    let initial = dir.0.join("initial.json");
    let output = dir.0.join("transformed.json");
    let rejected = dir.0.join("rejected.json");
    let operations = dir.0.join("operations.json");
    assert!(cli(&["new", "4", "3", initial.to_str().unwrap()]).status.success());
    fs::write(&operations, r#"[
        {"op":"pixel","layer":0,"x":0,"y":0,"color":[5,6,7,255]},
        {"op":"duplicate_layer","layer":0},
        {"op":"flip_layer","layer":1,"axis":"horizontal"},
        {"op":"flip_layer","layer":1,"axis":"vertical"},
        {"op":"clear_layer","layer":0}
    ]"#).unwrap();
    let result = cli(&["apply", initial.to_str().unwrap(), output.to_str().unwrap(), operations.to_str().unwrap()]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let project = perveance::Project::load(&output).unwrap();
    assert_eq!(project.layers.len(), 2);
    assert_eq!(project.layers[1].name, "Layer 1 copy");
    assert_eq!(project.layers[0].pixels[0], [0; 4]);
    assert_eq!(project.layers[1].pixels[11], [5, 6, 7, 255]);
    let png = dir.0.join("transformed.png");
    assert!(cli(&["export-png", output.to_str().unwrap(), png.to_str().unwrap()]).status.success());
    let mut reader = png::Decoder::new(fs::File::open(png).unwrap()).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut pixels).unwrap();
    assert_eq!(&pixels[44..48], &[5, 6, 7, 255]);
    assert_eq!(&pixels[0..4], &[0; 4]);
    fs::write(&operations, r#"[{"op":"duplicate_layer","layer":0},{"op":"clear_layer","layer":9}]"#).unwrap();
    let result = cli(&["apply", initial.to_str().unwrap(), rejected.to_str().unwrap(), operations.to_str().unwrap()]);
    assert!(!result.status.success());
    assert!(!rejected.exists());
    assert_eq!(perveance::Project::load(&initial).unwrap().layers.len(), 1);
    let capabilities: serde_json::Value = serde_json::from_slice(&cli(&["capabilities"]).stdout).unwrap();
    for operation in ["duplicate_layer", "clear_layer", "flip_layer"] {
        assert!(capabilities["batch_operations"].as_array().unwrap().iter().any(|value| value == operation));
        assert_eq!(capabilities["batch_examples"][operation]["op"], operation);
    }
}

#[test]
fn revision_checked_proposals_are_data_only_bounded_and_transactional() {
    let dir = TempDir::new();
    let input = dir.0.join("base.json");
    let output = dir.0.join("edited.json");
    let changed = dir.0.join("changed.json");
    let rejected = dir.0.join("rejected.json");
    let proposal = dir.0.join("proposal.json");
    let input_path = input.to_str().unwrap();
    let output_path = output.to_str().unwrap();
    let changed_path = changed.to_str().unwrap();
    let rejected_path = rejected.to_str().unwrap();
    let proposal_path = proposal.to_str().unwrap();
    assert!(cli(&["new", "3", "3", input_path]).status.success());
    let inspected: serde_json::Value = serde_json::from_slice(&cli(&["inspect", input_path]).stdout).unwrap();
    let revision = inspected["revision"].as_str().unwrap();
    assert!(revision.starts_with("sha256-json-v1:"));
    let proposal_json = serde_json::json!({"version":1,"base_revision":revision,"operations":[
        {"op":"add_layer"},
        {"op":"pixel","layer":1,"x":1,"y":1,"color":[3,4,5,255]}
    ]});
    fs::write(&proposal, serde_json::to_vec(&proposal_json).unwrap()).unwrap();
    let result = cli(&["apply-proposal", input_path, output_path, proposal_path]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let response: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(response["base_revision"], revision);
    assert_ne!(response["revision"], revision);
    assert_eq!(perveance::Project::load(&output).unwrap().layers[1].pixels[4], [3, 4, 5, 255]);
    assert!(cli(&["paint", input_path, changed_path, "0", "0", "0", "1", "1", "1", "255"]).status.success());
    let stale = cli(&["apply-proposal", changed_path, rejected_path, proposal_path]);
    assert!(!stale.status.success());
    let error: serde_json::Value = serde_json::from_slice(&stale.stderr).unwrap();
    assert!(error["error"].as_str().unwrap().contains("stale"));
    assert!(!rejected.exists());
    // No-clobber also applies to proposals, even when the revision is valid.
    let existing = fs::read(&output).unwrap();
    assert!(!cli(&["apply-proposal", input_path, output_path, proposal_path]).status.success());
    assert_eq!(fs::read(&output).unwrap(), existing);
    let mut invalid = proposal_json.clone();
    invalid["operations"][1]["layer"] = serde_json::json!(99);
    fs::write(&proposal, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(!cli(&["apply-proposal", input_path, rejected_path, proposal_path]).status.success());
    assert!(!rejected.exists());
    invalid["extra"] = serde_json::json!("unknown");
    fs::write(&proposal, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(!cli(&["apply-proposal", input_path, rejected_path, proposal_path]).status.success());
    fs::write(&proposal, vec![b' '; perveance::MAX_PROPOSAL_BYTES as usize + 1]).unwrap();
    let too_large = cli(&["apply-proposal", input_path, rejected_path, proposal_path]);
    assert!(!too_large.status.success());
    assert!(!rejected.exists());
}

#[test]
fn fingerprint_is_based_on_project_content_not_json_formatting() {
    let dir = TempDir::new();
    let compact = dir.0.join("compact.json");
    let pretty = dir.0.join("pretty.json");
    assert!(cli(&["new", "2", "2", compact.to_str().unwrap()]).status.success());
    let project: serde_json::Value = serde_json::from_slice(&fs::read(&compact).unwrap()).unwrap();
    fs::write(&pretty, serde_json::to_vec_pretty(&project).unwrap()).unwrap();
    let a: serde_json::Value = serde_json::from_slice(&cli(&["inspect", compact.to_str().unwrap()]).stdout).unwrap();
    let b: serde_json::Value = serde_json::from_slice(&cli(&["inspect", pretty.to_str().unwrap()]).stdout).unwrap();
    assert_eq!(a["revision"], b["revision"]);
}

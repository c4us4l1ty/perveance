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
    assert_eq!(value["autonomy_status"]["state"], "disabled");
    assert_eq!(value["autonomy_status"]["diagnostic_can_enable_execution"], false);
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

#[test]
fn read_region_exposes_revision_bound_rows_and_validates_bounds() {
    let dir = TempDir::new();
    let initial = dir.0.join("base.json");
    let edited = dir.0.join("edited.json");
    let operations = dir.0.join("ops.json");
    assert!(cli(&["new", "65", "64", initial.to_str().unwrap()]).status.success());
    fs::write(&operations, r#"[
        {"op":"pixel","layer":0,"x":2,"y":1,"color":[20,40,60,200]},
        {"op":"pixel","layer":0,"x":3,"y":1,"color":[9,8,7,0]},
        {"op":"add_layer"},
        {"op":"pixel","layer":1,"x":2,"y":1,"color":[200,120,50,100]},
        {"op":"set_visibility","layer":1,"visible":false}
    ]"#).unwrap();
    let edit = cli(&["apply", initial.to_str().unwrap(), edited.to_str().unwrap(), operations.to_str().unwrap()]);
    assert!(edit.status.success(), "{}", String::from_utf8_lossy(&edit.stderr));
    let path = edited.to_str().unwrap();
    let before = fs::read(&edited).unwrap();
    let raw = cli(&["read-region", path, "2", "1", "2", "2", "layer", "1"]);
    assert!(raw.status.success(), "{}", String::from_utf8_lossy(&raw.stderr));
    let raw: serde_json::Value = serde_json::from_slice(&raw.stdout).unwrap();
    assert_eq!(raw["pixels"], serde_json::json!([[[200,120,50,100],[0,0,0,0]],[[0,0,0,0],[0,0,0,0]]]));
    assert_eq!(raw["layer"], 1);
    let transparent = cli(&["read-region", path, "3", "1", "1", "1", "layer", "0"]);
    assert!(transparent.status.success());
    let transparent: serde_json::Value = serde_json::from_slice(&transparent.stdout).unwrap();
    assert_eq!(transparent["pixels"][0][0], serde_json::json!([9,8,7,0]));
    let composite = cli(&["read-region", path, "2", "1", "2", "2", "composite"]);
    assert!(composite.status.success());
    let composite: serde_json::Value = serde_json::from_slice(&composite.stdout).unwrap();
    assert_eq!(composite["pixels"][0][0], serde_json::json!([20,40,60,200]));
    assert_eq!(composite["layer"], serde_json::Value::Null);
    let inspect: serde_json::Value = serde_json::from_slice(&cli(&["inspect", path]).stdout).unwrap();
    assert_eq!(raw["revision"], inspect["revision"]);
    assert_eq!(composite["revision"], inspect["revision"]);
    for (y, row) in composite["pixels"].as_array().unwrap().iter().enumerate() {
        for (x, color) in row.as_array().unwrap().iter().enumerate() {
            let sample = cli(&["sample", path, &(x + 2).to_string(), &(y + 1).to_string(), "composite"]);
            let sample: serde_json::Value = serde_json::from_slice(&sample.stdout).unwrap();
            assert_eq!(*color, sample["color"]);
        }
    }
    let caps: serde_json::Value = serde_json::from_slice(&cli(&["capabilities"]).stdout).unwrap();
    assert_eq!(caps["limits"]["region_pixels"], 4096);
    assert!(caps["operations"].as_array().unwrap().iter().any(|v| v == "read-region"));
    assert!(cli(&["read-region", path, "0", "0", "64", "64", "composite"]).status.success());
    for args in [
        vec!["read-region", path, "0", "0", "65", "64", "composite"],
        vec!["read-region", path, "0", "0", "0", "1", "composite"],
        vec!["read-region", path, "64", "0", "2", "1", "composite"],
        vec!["read-region", path, "4294967295", "0", "1", "1", "composite"],
        vec!["read-region", path, "0", "0", "4294967295", "2", "composite"],
        vec!["read-region", path, "0", "0", "1", "1", "layer", "2"],
    ] {
        let result = cli(&args);
        assert!(!result.status.success(), "unexpected success: {:?}", args);
        assert!(result.stdout.is_empty());
        let err: serde_json::Value = serde_json::from_slice(&result.stderr).unwrap();
        assert_eq!(err["ok"], false);
    }
    assert_eq!(fs::read(edited).unwrap(), before);
}

#[test]
fn inspect_palette_reports_colors_and_rejects_invalid_files() {
    let dir = TempDir::new();
    let path = dir.0.join("palette.json");
    let mut colors = [[0; 4]; perveance::PALETTE_SLOTS];
    colors[3] = [7, 8, 9, 12];
    perveance::Palette::new(colors).save_new(&path).unwrap();
    let output = cli(&["inspect-palette", path.to_str().unwrap()]);
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["version"], 1);
    assert_eq!(value["colors"][3], serde_json::json!([7,8,9,12]));
    let caps: serde_json::Value = serde_json::from_slice(&cli(&["capabilities"]).stdout).unwrap();
    assert!(caps["operations"].as_array().unwrap().iter().any(|v| v == "inspect-palette"));
    fs::write(&path, b"{}").unwrap();
    let failed = cli(&["inspect-palette", path.to_str().unwrap()]);
    assert!(!failed.status.success());
    let error: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
    assert_eq!(error["ok"], false);
}

#[test]
fn translation_is_discoverable_and_proposals_require_explicit_clipping() {
    let dir = TempDir::new();
    let input = dir.0.join("input.json");
    let moved = dir.0.join("moved.json");
    let rejected = dir.0.join("rejected.json");
    let proposal = dir.0.join("proposal.json");
    let mut project = perveance::Project::new(3, 2).unwrap();
    project.set_pixel(0, 0, 0, [1, 2, 3, 0]).unwrap();
    project.set_pixel(0, 1, 0, [4, 5, 6, 255]).unwrap();
    project.save_new(&input).unwrap();
    let caps: serde_json::Value = serde_json::from_slice(&cli(&["capabilities"]).stdout).unwrap();
    assert!(caps["batch_operations"].as_array().unwrap().iter().any(|v| v == "translate_layer"));
    let revision = project.revision().unwrap();
    fs::write(&proposal, serde_json::to_vec(&serde_json::json!({"version": 1, "base_revision": revision,
        "operations": [{"op":"translate_layer", "layer":0, "dx":1, "dy":1, "clip":false}]})).unwrap()).unwrap();
    let args = ["apply-proposal", input.to_str().unwrap(), moved.to_str().unwrap(), proposal.to_str().unwrap()];
    let result = cli(&args);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let result = perveance::Project::load(&moved).unwrap();
    assert_eq!(result.sample_layer(0, 1, 1).unwrap(), [1, 2, 3, 0]);
    assert_eq!(result.sample_layer(0, 2, 1).unwrap(), [4, 5, 6, 255]);
    assert_eq!(perveance::Project::load(&input).unwrap(), project);
    let revision = result.revision().unwrap();
    for operation in [
        serde_json::json!({"op":"translate_layer", "layer":0, "dx":1, "dy":0, "clip":false}),
        serde_json::json!({"op":"translate_layer", "layer":0, "dx":1, "dy":0}),
        serde_json::json!({"op":"translate_layer", "layer":0, "dx":1, "dy":0, "clip":true, "extra": 1}),
    ] {
        fs::write(&proposal, serde_json::to_vec(&serde_json::json!({"version": 1, "base_revision": revision, "operations": [operation]})).unwrap()).unwrap();
        let attempt = cli(&["apply-proposal", moved.to_str().unwrap(), rejected.to_str().unwrap(), proposal.to_str().unwrap()]);
        assert!(!attempt.status.success());
        assert!(!rejected.exists());
    }
    assert_eq!(perveance::Project::load(moved.as_path()).unwrap(), result);
}

#[test]
fn resize_proposal_and_export_preserve_layers_and_refuse_unapproved_crop() {
    let dir = TempDir::new();
    let input = dir.0.join("input.json");
    let resized = dir.0.join("resized.json");
    let invalid = dir.0.join("invalid.json");
    let proposal = dir.0.join("proposal.json");
    let png = dir.0.join("resized.png");
    let mut project = perveance::Project::new(2, 2).unwrap();
    project.set_pixel(0, 1, 1, [33, 44, 55, 255]).unwrap();
    project.add_layer().unwrap();
    project.layers[1].visible = false;
    project.set_pixel(1, 0, 1, [7, 8, 9, 0]).unwrap();
    project.save_new(&input).unwrap();
    let revision = project.revision().unwrap();
    let caps: serde_json::Value = serde_json::from_slice(&cli(&["capabilities"]).stdout).unwrap();
    assert!(caps["batch_operations"].as_array().unwrap().iter().any(|v| v == "resize_canvas"));
    for op in [serde_json::json!({"op":"resize_canvas","width":1,"height":1,"clip":false}),
        serde_json::json!({"op":"resize_canvas","width":3,"height":3}),
        serde_json::json!({"op":"resize_canvas","width":3,"height":3,"clip":true,"extra":0})] {
        fs::write(&proposal, serde_json::to_vec(&serde_json::json!({"version":1,"base_revision":revision,"operations":[op]})).unwrap()).unwrap();
        let result = cli(&["apply-proposal", input.to_str().unwrap(), invalid.to_str().unwrap(), proposal.to_str().unwrap()]);
        assert!(!result.status.success());
        assert!(!invalid.exists());
    }
    fs::write(&proposal, serde_json::to_vec(&serde_json::json!({"version":1,"base_revision":revision,"operations":[
        {"op":"resize_canvas","width":3,"height":3,"clip":false},
        {"op":"pixel","layer":0,"x":2,"y":2,"color":[99,88,77,255]}]})).unwrap()).unwrap();
    let result = cli(&["apply-proposal", input.to_str().unwrap(), resized.to_str().unwrap(), proposal.to_str().unwrap()]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let output = perveance::Project::load(&resized).unwrap();
    assert_eq!((output.width, output.height), (3,3));
    assert_eq!(output.sample_layer(1, 0, 1).unwrap(), [7,8,9,0]);
    assert!(!output.layers[1].visible);
    assert_eq!(perveance::Project::load(&input).unwrap(), project);
    assert!(cli(&["export-png", resized.to_str().unwrap(), png.to_str().unwrap()]).status.success());
    let mut reader = png::Decoder::new(fs::File::open(png).unwrap()).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!((info.width, info.height), (3, 3));
    assert_eq!(&pixels[(2 * 3 + 2) * 4..(2 * 3 + 3) * 4], &[99,88,77,255]);
}

#[test]
fn openrouter_cli_preflight_is_opt_in_and_does_not_leak_secret() {
    use std::io::Write;
    use std::process::Stdio;
    fn invoke(args: &[&str], key: &[u8]) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_perveance-cli")).args(args)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(key).unwrap();
        child.wait_with_output().unwrap()
    }
    let dir = TempDir::new();
    let project = dir.0.join("base.json");
    let output = dir.0.join("out.json");
    let prompt = dir.0.join("prompt.txt");
    assert!(cli(&["new", "2", "2", project.to_str().unwrap()]).status.success());
    fs::write(&prompt, "draw a private test scene").unwrap();
    let run = |model: &str, secret: &[u8]| invoke(&["prompt-openrouter", project.to_str().unwrap(), output.to_str().unwrap(), model, prompt.to_str().unwrap()], secret);
    let result = run("invalid model", b"private-fake-key\n");
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    let message = String::from_utf8(result.stderr).unwrap();
    assert!(!message.contains("private-fake-key"));
    assert!(!message.contains("private test scene"));
    assert!(!output.exists());
    let flagged = invoke(&["prompt-openrouter", project.to_str().unwrap(), output.to_str().unwrap(), "invalid model", prompt.to_str().unwrap(), "--include-composite"], b"private-fake-key\n");
    assert!(!flagged.status.success());
    let flagged_error = String::from_utf8(flagged.stderr).unwrap();
    assert!(flagged_error.contains("invalid model ID"));
    assert!(!flagged_error.contains("private-fake-key"));
    assert!(!output.exists());
    let unknown = Command::new(env!("CARGO_BIN_EXE_perveance-cli")).args(["prompt-openrouter", project.to_str().unwrap(), output.to_str().unwrap(), "test/model", prompt.to_str().unwrap(), "--typo"]).output().unwrap();
    assert!(!unknown.status.success());
    assert!(!output.exists());
    let invalid = run("invalid model", &[b'x'; 513]);
    assert!(!invalid.status.success());
    assert!(!output.exists());
    fs::write(&prompt, vec![b'x'; perveance::provider::MAX_PROMPT_BYTES + 1]).unwrap();
    // Prompt is rejected before the key is read or any network request starts.
    let result = Command::new(env!("CARGO_BIN_EXE_perveance-cli")).args(["prompt-openrouter", project.to_str().unwrap(), output.to_str().unwrap(), "test/model", prompt.to_str().unwrap()]).output().unwrap();
    assert!(!result.status.success());
    assert!(!output.exists());
    let caps: serde_json::Value = serde_json::from_slice(&cli(&["capabilities"]).stdout).unwrap();
    assert!(caps["operations"].as_array().unwrap().iter().any(|v| v == "prompt-openrouter"));
    assert_eq!(caps["autonomous_execution"], false);
    assert_eq!(caps["experimental_prompt_openrouter"]["max_composite_pixels"], 1024);
    assert_eq!(caps["experimental_prompt_openrouter"]["visible_composite_opt_in"], "--include-composite");
}

#[test]
fn cli_scaled_export_reports_dimensions_and_never_overwrites() {
    let dir = TempDir::new();
    let project = dir.0.join("base.json");
    let png = dir.0.join("scaled.png");
    let invalid = dir.0.join("invalid.png");
    let p = perveance::Project::new(3, 2).unwrap();
    p.save_new(&project).unwrap();
    let result = cli(&["export-png-scaled", project.to_str().unwrap(), png.to_str().unwrap(), "3"]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!((value["width"].as_u64(), value["height"].as_u64(), value["scale"].as_u64()), (Some(9), Some(6), Some(3)));
    let before = fs::read(&png).unwrap();
    assert!(!cli(&["export-png-scaled", project.to_str().unwrap(), png.to_str().unwrap(), "2"]).status.success());
    assert_eq!(fs::read(&png).unwrap(), before);
    for scale in ["0", "9", "-1", "4294967295", "nan"] {
        assert!(!cli(&["export-png-scaled", project.to_str().unwrap(), invalid.to_str().unwrap(), scale]).status.success());
        assert!(!invalid.exists());
    }
    let caps: serde_json::Value = serde_json::from_slice(&cli(&["capabilities"]).stdout).unwrap();
    assert_eq!(caps["limits"]["png_export_scale"], 8);
    assert!(caps["operations"].as_array().unwrap().iter().any(|v| v == "export-png-scaled"));
    assert_eq!(perveance::Project::load(&project).unwrap(), p);
}

#[test]
fn brush_operations_are_discoverable_transactional_and_exportable() {
    let dir = TempDir::new();
    let initial = dir.0.join("initial.json");
    let output = dir.0.join("output.json");
    let bad = dir.0.join("bad.json");
    let proposal = dir.0.join("proposal.json");
    let png = dir.0.join("brush.png");
    let p = perveance::Project::new(5, 5).unwrap();
    p.save_new(&initial).unwrap();
    let revision = p.revision().unwrap();
    let ops = serde_json::json!([{"op":"brush_line", "layer":0, "from":[0,2], "to":[4,2], "size":3, "color":[10,20,30,255]}]);
    fs::write(&proposal, serde_json::to_vec(&serde_json::json!({"version":1,"base_revision":revision,"operations":ops})).unwrap()).unwrap();
    let result = cli(&["apply-proposal", initial.to_str().unwrap(), output.to_str().unwrap(), proposal.to_str().unwrap()]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let edited = perveance::Project::load(&output).unwrap();
    assert_eq!(edited.sample_layer(0, 4, 1).unwrap(), [10,20,30,255]);
    assert_eq!(edited.sample_layer(0, 4, 4).unwrap(), [0;4]);
    assert!(cli(&["export-png", output.to_str().unwrap(), png.to_str().unwrap()]).status.success());
    let mut reader = png::Decoder::new(fs::File::open(png).unwrap()).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut pixels).unwrap();
    assert_eq!(&pixels[(5 + 4) * 4..(5 + 5) * 4], &[10,20,30,255]);
    let bad_op = serde_json::json!({"op":"brush_line", "layer":0, "from":[0,0], "to":[1,1], "size":17, "color":[1,2,3,4]});
    fs::write(&proposal, serde_json::to_vec(&serde_json::json!({"version":1,"base_revision":revision,"operations":[bad_op]})).unwrap()).unwrap();
    assert!(!cli(&["apply-proposal", initial.to_str().unwrap(), bad.to_str().unwrap(), proposal.to_str().unwrap()]).status.success());
    assert!(!bad.exists());
    assert_eq!(perveance::Project::load(&initial).unwrap(), p);
    let caps: serde_json::Value = serde_json::from_slice(&cli(&["capabilities"]).stdout).unwrap();
    assert_eq!(caps["limits"]["brush_size"], 16);
    assert!(caps["batch_operations"].as_array().unwrap().iter().any(|v| v == "brush_line"));
}

#[test]
fn color_replacement_proposal_is_discoverable_atomic_and_preserves_source() {
    let dir = TempDir::new();
    let initial = dir.0.join("initial.json");
    let output = dir.0.join("edited.json");
    let bad = dir.0.join("bad.json");
    let proposal = dir.0.join("proposal.json");
    let mut p = perveance::Project::new(2, 2).unwrap();
    p.set_pixel(0, 0, 0, [1,2,3,0]).unwrap();
    p.set_pixel(0, 1, 0, [1,2,3,0]).unwrap();
    p.save_new(&initial).unwrap();
    let revision = p.revision().unwrap();
    fs::write(&proposal, serde_json::to_vec(&serde_json::json!({"version":1,"base_revision":revision,"operations":[
        {"op":"replace_color","layer":0,"from":[1,2,3,0],"to":[20,30,40,128]}]})).unwrap()).unwrap();
    let result = cli(&["apply-proposal", initial.to_str().unwrap(), output.to_str().unwrap(), proposal.to_str().unwrap()]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let edited = perveance::Project::load(&output).unwrap();
    assert_eq!(edited.sample_layer(0, 0, 0).unwrap(), [20,30,40,128]);
    assert_eq!(edited.sample_layer(0, 1, 0).unwrap(), [20,30,40,128]);
    assert_eq!(edited.sample_layer(0, 0, 1).unwrap(), [0;4]);
    assert_eq!(perveance::Project::load(&initial).unwrap(), p);
    for operation in [
        serde_json::json!({"op":"replace_color","layer":99,"from":[0,0,0,0],"to":[1,2,3,4]}),
        serde_json::json!({"op":"replace_color","layer":0,"from":[1,2,3,256],"to":[1,2,3,4]}),
        serde_json::json!({"op":"replace_color","layer":0,"from":[0,0,0,0],"to":[1,2,3,4],"extra":true}),
    ] {
        fs::write(&proposal, serde_json::to_vec(&serde_json::json!({"version":1,"base_revision":revision,"operations":[operation]})).unwrap()).unwrap();
        assert!(!cli(&["apply-proposal", initial.to_str().unwrap(), bad.to_str().unwrap(), proposal.to_str().unwrap()]).status.success());
        assert!(!bad.exists());
    }
    let caps: serde_json::Value = serde_json::from_slice(&cli(&["capabilities"]).stdout).unwrap();
    assert!(caps["batch_operations"].as_array().unwrap().iter().any(|v| v == "replace_color"));
}

#[cfg(target_os = "linux")]
#[test]
fn staging_diagnostic_copies_only_project_and_cleans_up() {
    let dir = TempDir::new();
    let parent = dir.0.join("stage-parent");
    fs::create_dir(&parent).unwrap();
    let input = dir.0.join("project.json");
    let mut p = perveance::Project::new(2, 2).unwrap();
    p.set_pixel(0, 1, 1, [7,8,9,10]).unwrap();
    p.save_new(&input).unwrap();
    let sentinel = parent.join("host-secret");
    fs::write(&sentinel, b"not project data").unwrap();
    let result = cli(&["verify-project-staging", input.to_str().unwrap(), parent.to_str().unwrap()]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["revision"], p.revision().unwrap());
    assert_eq!(value["staged_files"], 1);
    assert_eq!(value["persistent"], false);
    assert_eq!(value["autonomous_execution"], false);
    assert_eq!(value["security_boundary"], false);
    assert_eq!(fs::read_dir(&parent).unwrap().count(), 1);
    assert_eq!(fs::read(&sentinel).unwrap(), b"not project data");
    let caps: serde_json::Value = serde_json::from_slice(&cli(&["capabilities"]).stdout).unwrap();
    assert_eq!(caps["project_staging"]["security_boundary"], false);
    assert!(caps["operations"].as_array().unwrap().iter().any(|v| v == "verify-project-staging"));
    fs::write(&input, b"not json").unwrap();
    let failed = cli(&["verify-project-staging", input.to_str().unwrap(), parent.to_str().unwrap()]);
    assert!(!failed.status.success());
    assert_eq!(fs::read_dir(&parent).unwrap().count(), 1);
    fs::remove_file(&input).unwrap();
    p.save_new(&input).unwrap();
    let missing = cli(&["verify-project-staging", input.to_str().unwrap(), parent.join("missing").to_str().unwrap()]);
    assert!(!missing.status.success());
    assert_eq!(fs::read_dir(&parent).unwrap().count(), 1);
}

#[test]
fn autonomy_intake_is_strict_revision_bound_and_cannot_start_a_job() {
    let dir = TempDir::new();
    let project = dir.0.join("input.json");
    let req = dir.0.join("request.json");
    let policy = dir.0.join("policy.json");
    let p = perveance::Project::new(1, 1).unwrap();
    p.save_new(&project).unwrap();
    let request = serde_json::json!({"version": 1, "base_revision": p.revision().unwrap(), "prompt": "private task text", "budgets": {
        "wall_seconds": 10, "cpu_seconds": 10, "memory_mib": 32, "scratch_mib": 32,
        "processes": 2, "model_calls": 1, "model_cost_microusd": 1000
    }});
    fs::write(&req, serde_json::to_vec(&request).unwrap()).unwrap();
    fs::write(&policy, serde_json::to_vec(&perveance::supervisor::IntakePolicy::default()).unwrap()).unwrap();
    let before = fs::read(&project).unwrap();
    let result = cli(&["inspect-autonomy-request", project.to_str().unwrap(), req.to_str().unwrap(), policy.to_str().unwrap()]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["ok"], true);
    assert_eq!(report["intake"]["status"], "blocked");
    assert_eq!(report["intake"]["autonomous_execution"], false);
    assert_eq!(report["intake"]["budgets_enforced"], false);
    assert_eq!(report["intake"]["persistent_task_created"], false);
    assert!(!String::from_utf8_lossy(&result.stdout).contains("private task text"));
    assert_eq!(fs::read(&project).unwrap(), before);
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 3);
    assert!(cli(&["inspect-autonomy-request", project.to_str().unwrap(), req.to_str().unwrap()]).status.success());
    let mut stale = request.clone(); stale["base_revision"] = "stale".into();
    fs::write(&req, serde_json::to_vec(&stale).unwrap()).unwrap();
    assert!(!cli(&["inspect-autonomy-request", project.to_str().unwrap(), req.to_str().unwrap()]).status.success());
    let mut attack = request; attack["enable_sandbox"] = true.into();
    fs::write(&req, serde_json::to_vec(&attack).unwrap()).unwrap();
    assert!(!cli(&["inspect-autonomy-request", project.to_str().unwrap(), req.to_str().unwrap()]).status.success());
    let caps: serde_json::Value = serde_json::from_slice(&cli(&["capabilities"]).stdout).unwrap();
    assert_eq!(caps["autonomy_intake"]["budgets_enforced"], false);
    assert_eq!(caps["autonomous_execution"], false);
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires a compatible unprivileged bubblewrap host"]
fn fixed_worker_cli_on_supported_host() {
    let dir = TempDir::new();
    let project = dir.0.join("input.json");
    let parent = dir.0.join("scratch");
    fs::create_dir(&parent).unwrap();
    let p = perveance::Project::new(2, 3).unwrap();
    p.save_new(&project).unwrap();
    let before = fs::read(&project).unwrap();
    let result = cli(&["verify-fixed-worker", project.to_str().unwrap(), parent.to_str().unwrap()]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["revision"], p.revision().unwrap());
    assert_eq!(report["fixed_code_only"], true);
    assert_eq!(report["autonomous_execution"], false);
    assert_eq!(report["security_boundary"], false);
    assert_eq!(fs::read(project).unwrap(), before);
    assert_eq!(fs::read_dir(parent).unwrap().count(), 0);
}

#[test]
fn region_batches_and_revision_proposals_are_no_clobber_and_strict() {
    let dir = TempDir::new();
    let base = dir.0.join("base.json");
    let output = dir.0.join("edited.json");
    let rejected = dir.0.join("rejected.json");
    let ops = dir.0.join("operations.json");
    let proposal = dir.0.join("proposal.json");
    let mut p = perveance::Project::new(4, 3).unwrap();
    p.set_pixel(0, 0, 0, [7, 8, 9, 0]).unwrap();
    p.set_pixel(0, 1, 0, [10, 20, 30, 128]).unwrap();
    p.layers[0].visible = false;
    p.add_layer().unwrap();
    p.rectangle(1, 0, 0, 4, 3, [255; 4]).unwrap();
    p.save_new(&base).unwrap();
    let batch = serde_json::json!([
        {"op":"copy_region","layer":0,"x":0,"y":0,"width":2,"height":1,"to_layer":1,"to_x":2,"to_y":2},
        {"op":"clear_region","layer":0,"x":0,"y":0,"width":2,"height":1}
    ]);
    fs::write(&ops, serde_json::to_vec(&batch).unwrap()).unwrap();
    fs::write(&proposal, serde_json::to_vec(&serde_json::json!({"version":1,"base_revision":p.revision().unwrap(),"operations":batch})).unwrap()).unwrap();
    let result = cli(&["apply-proposal", base.to_str().unwrap(), output.to_str().unwrap(), proposal.to_str().unwrap()]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let edited = perveance::Project::load(&output).unwrap();
    assert_eq!(edited.sample_layer(1, 2, 2).unwrap(), [7, 8, 9, 0]);
    assert_eq!(edited.sample_layer(1, 3, 2).unwrap(), [10, 20, 30, 128]);
    assert_eq!(edited.sample_layer(0, 0, 0).unwrap(), [0; 4]);
    assert_eq!(perveance::Project::load(&base).unwrap(), p);
    let bytes = fs::read(&output).unwrap();
    assert!(!cli(&["apply", base.to_str().unwrap(), output.to_str().unwrap(), ops.to_str().unwrap()]).status.success());
    assert_eq!(fs::read(&output).unwrap(), bytes);
    assert!(!cli(&["apply-proposal", output.to_str().unwrap(), rejected.to_str().unwrap(), proposal.to_str().unwrap()]).status.success());
    assert!(!rejected.exists());
    for bad in [
        r#"[{"op":"clear_region","layer":0,"x":0,"y":0,"width":1,"height":1,"clip":true}]"#,
        r#"[{"op":"copy_region","layer":0,"x":0,"y":0,"width":1,"height":1,"to_layer":1,"to_x":0}]"#,
        r#"[{"op":"copy_region","layer":0,"x":0,"y":0,"width":2,"height":1,"to_layer":1,"to_x":3,"to_y":0}]"#,
        r#"[{"op":"clear_region","layer":0,"x":0,"y":0,"width":1,"height":1},{"op":"clear_region","layer":2,"x":0,"y":0,"width":1,"height":1}]"#,
    ] {
        fs::write(&ops, bad).unwrap();
        assert!(!cli(&["apply", base.to_str().unwrap(), rejected.to_str().unwrap(), ops.to_str().unwrap()]).status.success());
        assert!(!rejected.exists());
        assert_eq!(perveance::Project::load(&base).unwrap(), p);
    }
    let caps: serde_json::Value = serde_json::from_slice(&cli(&["capabilities"]).stdout).unwrap();
    for operation in ["clear_region", "copy_region"] {
        assert!(caps["batch_operations"].as_array().unwrap().iter().any(|value| value == operation));
        assert_eq!(caps["batch_examples"][operation]["op"], operation);
        let _: perveance::Operation = serde_json::from_value(caps["batch_examples"][operation].clone()).unwrap();
    }
}

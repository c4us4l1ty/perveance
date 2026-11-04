use perveance::Project;
use std::{path::Path, process::Command};

fn python(args: &[&str]) {
    let result = Command::new("python3").arg("tools/arena_art.py").args(args).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
}
fn decoded(path: &Path) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(std::fs::File::open(path).unwrap()).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(frame.color_type, png::ColorType::Rgba);
    (frame.width, frame.height, pixels[..frame.buffer_size()].to_vec())
}

#[test]
fn bridge_project_matches_rust_batches_and_png_compositor() {
    let dir = tempfile::tempdir().unwrap();
    let py_project = dir.path().join("python.json");
    let rust_project = dir.path().join("rust.json");
    let png = dir.path().join("python.png");
    let base = dir.path().join("base.json");
    python(&["create", "16", "16", "examples/potion-operations.json", py_project.to_str().unwrap()]);
    let exe = env!("CARGO_BIN_EXE_perveance-cli");
    assert!(Command::new(exe).args(["new", "16", "16", base.to_str().unwrap()]).status().unwrap().success());
    assert!(Command::new(exe).args(["apply", base.to_str().unwrap(), rust_project.to_str().unwrap(), "examples/potion-operations.json"]).status().unwrap().success());
    let p = Project::load(&py_project).unwrap();
    assert_eq!(p, Project::load(&rust_project).unwrap());
    python(&["export-png", py_project.to_str().unwrap(), png.to_str().unwrap()]);
    assert_eq!(decoded(&png), (16, 16, p.rgba().unwrap()));
}

#[test]
fn bridge_reads_rust_project_with_transparency_and_hidden_layers() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("rust.json");
    let output = dir.path().join("bridge.png");
    let mut p = Project::new(5, 5).unwrap();
    for y in 0..5 { for x in 0..5 {
        p.set_pixel(0, x, y, [(x * 51) as u8, (y * 51) as u8, 83, ((x + y) * 30) as u8]).unwrap();
    }}
    p.add_layer().unwrap();
    for y in 0..5 { for x in 0..5 {
        p.set_pixel(1, x, y, [30, (x * 47) as u8, (y * 31) as u8, ((x * 3 + y) * 17) as u8]).unwrap();
    }}
    p.add_layer().unwrap();
    p.set_pixel(2, 0, 0, [44, 55, 66, 0]).unwrap();
    p.layers[2].visible = false;
    p.save_new(&input).unwrap();
    python(&["export-png", input.to_str().unwrap(), output.to_str().unwrap()]);
    assert_eq!(decoded(&output), (5, 5, p.rgba().unwrap()));
}

#[test]
fn bridge_edits_match_rust_for_lines_fill_and_visibility() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base.json");
    let operations = dir.path().join("ops.json");
    let py = dir.path().join("py.json");
    let rust = dir.path().join("rust.json");
    let mut initial = Project::new(8, 8).unwrap();
    initial.set_pixel(0, 0, 0, [3, 4, 5, 0]).unwrap();
    initial.save_new(&base).unwrap();
    let mut ops = vec![
        serde_json::json!({"op":"fill", "layer":0, "x":7, "y":7, "color":[10,20,30,180]}),
        serde_json::json!({"op":"add_layer"}),
        serde_json::json!({"op":"rename_layer", "layer":1, "name":"Ink ✦"}),
    ];
    for x in 0..8 { for y in 0..8 {
        ops.push(serde_json::json!({"op":"line", "layer":1, "from":[x, 0], "to":[7-x, y], "color":[x*20,y*20,91,(x+y)*17]}));
    }}
    ops.push(serde_json::json!({"op":"set_visibility", "layer":1, "visible":false}));
    std::fs::write(&operations, serde_json::to_vec(&ops).unwrap()).unwrap();
    python(&["apply", base.to_str().unwrap(), py.to_str().unwrap(), operations.to_str().unwrap()]);
    let exe = env!("CARGO_BIN_EXE_perveance-cli");
    assert!(Command::new(exe).args(["apply", base.to_str().unwrap(), rust.to_str().unwrap(), operations.to_str().unwrap()]).status().unwrap().success());
    let final_project = Project::load(&py).unwrap();
    assert_eq!(final_project, Project::load(&rust).unwrap());
    let inspection = Command::new("python3").args(["tools/arena_art.py", "inspect", py.to_str().unwrap()]).output().unwrap();
    assert!(inspection.status.success());
    let report: serde_json::Value = serde_json::from_slice(&inspection.stdout).unwrap();
    assert_eq!(report["revision"], final_project.revision().unwrap());
    assert_eq!(Project::load(&base).unwrap(), initial);
}

#[test]
fn bridge_proposal_checks_rust_revision_and_matches_rust_output() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base.json");
    let proposal = dir.path().join("proposal.json");
    let py = dir.path().join("py.json");
    let rust = dir.path().join("rust.json");
    let mut project = Project::new(3, 3).unwrap();
    project.rename_layer(0, "é雪\n\"\u{0001}").unwrap();
    project.set_pixel(0, 0, 0, [1, 2, 3, 0]).unwrap();
    project.save_new(&base).unwrap();
    let content = serde_json::json!({"version":1,"base_revision":project.revision().unwrap(),
        "operations":[{"op":"pixel","layer":0,"x":1,"y":2,"color":[8,9,10,11]}]});
    std::fs::write(&proposal, serde_json::to_vec(&content).unwrap()).unwrap();
    python(&["apply-proposal", base.to_str().unwrap(), py.to_str().unwrap(), proposal.to_str().unwrap()]);
    let exe = env!("CARGO_BIN_EXE_perveance-cli");
    assert!(Command::new(exe).args(["apply-proposal", base.to_str().unwrap(), rust.to_str().unwrap(), proposal.to_str().unwrap()]).status().unwrap().success());
    assert_eq!(Project::load(&py).unwrap(), Project::load(&rust).unwrap());
    let mut stale = content;
    stale["base_revision"] = "sha256-json-v1:stale".into();
    std::fs::write(&proposal, serde_json::to_vec(&stale).unwrap()).unwrap();
    let bad = dir.path().join("bad.json");
    let result = Command::new("python3").args(["tools/arena_art.py", "apply-proposal", base.to_str().unwrap(), bad.to_str().unwrap(), proposal.to_str().unwrap()]).output().unwrap();
    assert!(!result.status.success());
    assert!(!bad.exists());
}

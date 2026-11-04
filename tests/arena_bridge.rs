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

#[test]
fn bridge_scaled_png_matches_rust_nearest_neighbor() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("project.json");
    let mut p = Project::new(3, 2).unwrap();
    p.set_pixel(0, 0, 0, [10, 20, 30, 255]).unwrap();
    p.set_pixel(0, 1, 1, [20, 40, 60, 127]).unwrap();
    p.add_layer().unwrap();
    p.set_pixel(1, 1, 1, [200, 100, 50, 90]).unwrap();
    p.save_new(&source).unwrap();
    for scale in [1, 2, 8] {
        let py_png = dir.path().join(format!("python-{scale}.png"));
        let rust_png = dir.path().join(format!("rust-{scale}.png"));
        python(&["export-png-scaled", source.to_str().unwrap(), py_png.to_str().unwrap(), &scale.to_string()]);
        p.export_png_scaled_new(&rust_png, scale).unwrap();
        assert_eq!(decoded(&py_png), decoded(&rust_png));
    }
    assert_eq!(Project::load(&source).unwrap(), p);
}

#[test]
fn bridge_brush_and_shapes_match_rust_on_edges_and_degenerate_bounds() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base.json");
    let ops = dir.path().join("ops.json");
    let bridge = dir.path().join("bridge.json");
    let rust = dir.path().join("rust.json");
    let mut project = Project::new(96, 80).unwrap();
    for _ in 0..4 { project.add_layer().unwrap(); }
    project.save_new(&base).unwrap();
    let mut batch = vec![];
    let mut tile = 0;
    for width in 1..=6 { for height in 1..=6 {
        for filled in [false, true] {
            batch.push(serde_json::json!({"op":"ellipse", "layer":0, "x":(tile%12)*8, "y":(tile/12)*8,
                "width":width, "height":height, "filled":filled, "color":[width*30,height*30,4,128]}));
            tile += 1;
        }
        batch.push(serde_json::json!({"op":"rectangle_outline", "layer":0, "x":(tile%12)*8, "y":(tile/12)*8,
            "width":width, "height":height, "color":[2,width*30,height*30,255]}));
        tile += 1;
    }}
    for (index, size) in [1, 2, 3, 16].into_iter().enumerate() {
        batch.push(serde_json::json!({"op":"brush_line", "layer":index+1, "from":[0,79], "to":[95,0], "size":size, "color":[size,3,4,0]}));
    }
    std::fs::write(&ops, serde_json::to_vec(&batch).unwrap()).unwrap();
    python(&["apply", base.to_str().unwrap(), bridge.to_str().unwrap(), ops.to_str().unwrap()]);
    let exe = env!("CARGO_BIN_EXE_perveance-cli");
    assert!(Command::new(exe).args(["apply", base.to_str().unwrap(), rust.to_str().unwrap(), ops.to_str().unwrap()]).status().unwrap().success());
    assert_eq!(Project::load(&bridge).unwrap(), Project::load(&rust).unwrap());
}

#[test]
fn bridge_read_region_matches_rust_and_rejects_invalid_regions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("region.json");
    let mut p = Project::new(70, 70).unwrap();
    p.set_pixel(0, 2, 3, [100, 20, 30, 128]).unwrap();
    p.add_layer().unwrap();
    p.set_pixel(1, 2, 3, [9, 8, 7, 0]).unwrap();
    p.layers[1].visible = false;
    p.save_new(&path).unwrap();
    let path = path.to_str().unwrap();
    let exe = env!("CARGO_BIN_EXE_perveance-cli");
    for tail in [vec!["composite"], vec!["layer", "1"]] {
        let mut args = vec!["read-region", path, "1", "2", "3", "3"];
        args.extend(tail);
        let py = Command::new("python3").arg("tools/arena_art.py").args(&args).output().unwrap();
        let rust = Command::new(exe).args(&args).output().unwrap();
        assert!(py.status.success(), "{}", String::from_utf8_lossy(&py.stderr));
        assert!(rust.status.success());
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&py.stdout).unwrap(), serde_json::from_slice::<serde_json::Value>(&rust.stdout).unwrap());
    }
    for tail in [vec!["0", "0", "65", "64", "composite"], vec!["69", "0", "2", "1", "composite"],
        vec!["0", "0", "1", "1", "layer", "2"], vec!["0", "0", "0", "1", "composite"]] {
        let mut args = vec!["read-region", path]; args.extend(tail);
        let py = Command::new("python3").arg("tools/arena_art.py").args(&args).output().unwrap();
        let rust = Command::new(exe).args(&args).output().unwrap();
        assert!(!py.status.success()); assert!(!rust.status.success());
    }
}

#[test]
fn bridge_layer_and_canvas_operations_match_rust_at_every_batch_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base.json");
    let ops = dir.path().join("ops.json");
    let exe = env!("CARGO_BIN_EXE_perveance-cli");
    let mut project = Project::new(4, 3).unwrap();
    project.rename_layer(0, &"雪".repeat(84)).unwrap();
    project.set_pixel(0, 1, 1, [12, 34, 56, 0]).unwrap();
    project.layers[0].visible = false;
    project.save_new(&base).unwrap();
    let batch = vec![
        serde_json::json!({"op":"duplicate_layer","layer":0}),
        serde_json::json!({"op":"flip_layer","layer":1,"axis":"horizontal"}),
        serde_json::json!({"op":"flip_layer","layer":1,"axis":"vertical"}),
        serde_json::json!({"op":"replace_color","layer":1,"from":[12,34,56,0],"to":[99,11,22,123]}),
        serde_json::json!({"op":"translate_layer","layer":1,"dx":-1,"dy":1,"clip":false}),
        serde_json::json!({"op":"resize_canvas","width":6,"height":5,"clip":false}),
        serde_json::json!({"op":"add_layer"}),
        serde_json::json!({"op":"pixel","layer":2,"x":5,"y":4,"color":[9,8,7,0]}),
        serde_json::json!({"op":"move_layer","from":2,"to":0}),
        serde_json::json!({"op":"translate_layer","layer":0,"dx":1,"dy":0,"clip":true}),
        serde_json::json!({"op":"clear_layer","layer":0}),
        serde_json::json!({"op":"resize_canvas","width":3,"height":2,"clip":true}),
        serde_json::json!({"op":"remove_layer","layer":1}),
    ];
    for end in 1..=batch.len() {
        let py = dir.path().join(format!("py-{end}.json"));
        let rust = dir.path().join(format!("rust-{end}.json"));
        std::fs::write(&ops, serde_json::to_vec(&batch[..end]).unwrap()).unwrap();
        python(&["apply", base.to_str().unwrap(), py.to_str().unwrap(), ops.to_str().unwrap()]);
        let result = Command::new(exe).args(["apply", base.to_str().unwrap(), rust.to_str().unwrap(), ops.to_str().unwrap()]).output().unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        assert_eq!(Project::load(&py).unwrap(), Project::load(&rust).unwrap(), "batch prefix {end}");
    }
}

#[test]
fn bridge_transform_failures_match_rust_without_creating_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base.json");
    let ops = dir.path().join("ops.json");
    let mut project = Project::new(2, 2).unwrap();
    project.set_pixel(0, 1, 1, [9, 0, 0, 0]).unwrap();
    project.layers[0].visible = false;
    project.save_new(&base).unwrap();
    let bad = vec![
        serde_json::json!({"op":"translate_layer","layer":0,"dx":1,"dy":0,"clip":false}),
        serde_json::json!({"op":"resize_canvas","width":1,"height":1,"clip":false}),
        serde_json::json!({"op":"resize_canvas","width":1,"height":1,"clip":1}),
        serde_json::json!({"op":"flip_layer","layer":0,"axis":"diagonal"}),
        serde_json::json!({"op":"remove_layer","layer":0}),
    ];
    for (index, operation) in bad.into_iter().enumerate() {
        let py = dir.path().join(format!("bad-py-{index}.json"));
        let rust = dir.path().join(format!("bad-rust-{index}.json"));
        std::fs::write(&ops, serde_json::to_vec(&vec![serde_json::json!({"op":"add_layer"}), serde_json::json!({"op":"remove_layer","layer":1}), operation]).unwrap()).unwrap();
        let a = Command::new("python3").arg("tools/arena_art.py").args(["apply", base.to_str().unwrap(), py.to_str().unwrap(), ops.to_str().unwrap()]).output().unwrap();
        let b = Command::new(env!("CARGO_BIN_EXE_perveance-cli")).args(["apply", base.to_str().unwrap(), rust.to_str().unwrap(), ops.to_str().unwrap()]).output().unwrap();
        assert!(!a.status.success()); assert!(!b.status.success());
        assert!(!py.exists()); assert!(!rust.exists());
        assert_eq!(Project::load(&base).unwrap(), project);
    }
}

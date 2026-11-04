use perveance::{provider, EditProposal, Operation, Palette, Project};
use serde_json::json;
use std::{env, fs::{self, File}, io::{self, IsTerminal, Read}, path::Path, process};

fn number(s: &str) -> Result<u32, String> { s.parse().map_err(|_| format!("invalid number: {s}")) }
fn byte(s: &str) -> Result<u8, String> { s.parse().map_err(|_| format!("invalid color channel: {s}")) }


// Bound output and work even if callers provide enormous or overflowing coordinates.
const MAX_REGION_PIXELS: u32 = 4096;

fn read_region(project: &Project, x: u32, y: u32, width: u32, height: u32, layer: Option<usize>) -> Result<serde_json::Value, String> {
    let count = width.checked_mul(height).ok_or("region size overflow")?;
    if count == 0 || count > MAX_REGION_PIXELS { return Err("region must contain 1 to 4096 pixels".into()); }
    if x.checked_add(width).filter(|&end| end <= project.width).is_none()
        || y.checked_add(height).filter(|&end| end <= project.height).is_none() {
        return Err("region is outside the canvas".into());
    }
    if let Some(index) = layer {
        if index >= project.layers.len() { return Err("invalid layer index".into()); }
    }
    let mut rows = Vec::with_capacity(height as usize);
    for py in y..y + height {
        let mut row = Vec::with_capacity(width as usize);
        for px in x..x + width {
            row.push(match layer {
                Some(index) => project.sample_layer(index, px, py)?,
                None => project.sample_composite(px, py)?,
            });
        }
        rows.push(row);
    }
    Ok(json!({"ok": true, "revision": project.revision()?, "x": x, "y": y,
        "width": width, "height": height, "source": if layer.is_some() { "layer" } else { "composite" },
        "layer": layer, "pixels": rows}))
}

// Reads one key from a pipe or file, not a visible terminal, command argument or env var.
// Accept an optional single LF/CRLF; no other whitespace is stripped.
fn read_key<R: Read>(reader: R) -> Result<String, String> {
    let mut bytes = Vec::new();
    reader.take((provider::MAX_API_KEY_BYTES + 3) as u64).read_to_end(&mut bytes)
        .map_err(|_| "could not read API key from stdin")?;
    if bytes.ends_with(b"\n") {
        bytes.pop();
        if bytes.ends_with(b"\r") { bytes.pop(); }
    }
    if bytes.len() > provider::MAX_API_KEY_BYTES { return Err("invalid API key format".into()); }
    String::from_utf8(bytes).map_err(|_| "invalid API key format".into())
}

fn read_prompt(path: &Path) -> Result<String, String> {
    let file = File::open(path).map_err(|e| format!("open prompt: {e}"))?;
    let mut bytes = Vec::new();
    file.take((provider::MAX_PROMPT_BYTES + 1) as u64).read_to_end(&mut bytes)
        .map_err(|e| format!("read prompt: {e}"))?;
    if bytes.len() > provider::MAX_PROMPT_BYTES { return Err("prompt exceeds size limit".into()); }
    String::from_utf8(bytes).map_err(|_| "prompt must be valid UTF-8".into())
}

fn prompt_to_project<F>(input: &Path, output: &Path, model: &str, prompt: &str, key: &str, include_composite: bool, request: F) -> Result<serde_json::Value, String>
where F: FnOnce(&provider::TaskContext, &str, &str, &str) -> Result<EditProposal, String> {
    provider::validate_request(model, key, prompt)?;
    // A preflight avoids an obvious charge on an occupied path. This is NOT a
    // reservation: a race or write failure may still occur after a paid request.
    match fs::symlink_metadata(output) {
        Ok(_) => return Err("output already exists".into()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {},
        Err(e) => return Err(format!("check output: {e}")),
    }
    let mut project = Project::load(input)?;
    let base_revision = project.revision()?;
    let context = if include_composite { provider::TaskContext::from_project_with_composite(&project)? } else { provider::TaskContext::from_project(&project)? };
    let proposal = request(&context, model, key, prompt)?;
    provider::apply_generated(&mut project, &proposal)?;
    let revision = project.revision()?;
    project.save_new(output)?;
    Ok(json!({"ok": true, "output": output.to_string_lossy(), "applied": proposal.operations.len(), "base_revision": base_revision, "revision": revision, "shared_composite": include_composite}))
}

fn prompt_openrouter(input: &str, output: &str, model: &str, prompt_path: &str, include_composite: bool) -> Result<serde_json::Value, String> {
    let prompt = read_prompt(Path::new(prompt_path))?;
    let stdin = io::stdin();
    if stdin.is_terminal() { return Err("API key must come from a pipe or redirected stdin, not a terminal".into()); }
    let key = read_key(stdin.lock())?;
    prompt_to_project(Path::new(input), Path::new(output), model, &prompt, &key, include_composite, provider::propose_openrouter)
}

fn inspect_autonomy_request(input: &str, request: &str, policy: Option<&str>) -> Result<serde_json::Value, String> {
    use perveance::supervisor::{inspect_request, IntakePolicy, TaskRequest};
    let policy = match policy { Some(path) => IntakePolicy::load(Path::new(path))?, None => IntakePolicy::default() };
    let request = TaskRequest::load(Path::new(request))?;
    let project = Project::load(Path::new(input))?;
    let report = inspect_request(&project, &request, &policy)?;
    Ok(json!({"ok": true, "intake": report}))
}

fn run(args: &[String]) -> Result<serde_json::Value, String> {
    match args {
        [command] if command == "capabilities" => Ok(json!({"api_version": 1, "prototype": true, "autonomous_execution": false,
            "autonomy_status": {"state": "disabled", "reason": "No verified supervisor or sandbox is implemented", "diagnostic": "tools/isolation_probe.py", "diagnostic_can_enable_execution": false},
            "operations": ["new", "new-from-png", "inspect", "inspect-palette", "verify-project-staging", "verify-fixed-worker", "inspect-autonomy-request", "sample", "read-region", "prompt-openrouter", "paint", "apply", "apply-proposal", "import-png", "export-png", "export-png-scaled"],
            "batch_operations": ["add_layer", "duplicate_layer", "clear_layer", "clear_region", "copy_region", "replace_color", "flip_layer", "translate_layer", "resize_canvas", "set_visibility", "rename_layer", "move_layer", "remove_layer", "pixel", "line", "brush_line", "rectangle", "rectangle_outline", "ellipse", "fill"],
            "limits": {"canvas_dimension": 256, "png_export_scale": perveance::MAX_EXPORT_SCALE, "brush_size": perveance::MAX_BRUSH_SIZE, "layers": 32, "region_pixels": MAX_REGION_PIXELS, "region_edit_pixels": perveance::MAX_DIMENSION * perveance::MAX_DIMENSION, "batch_operations": 1024, "batch_bytes": 262144, "proposal_bytes": perveance::MAX_PROPOSAL_BYTES, "png_bytes": perveance::MAX_PNG_BYTES},
            "batch_examples": {
                "rename_layer": {"op": "rename_layer", "layer": 0, "name": "Ink"},
                "move_layer": {"op": "move_layer", "from": 0, "to": 1},
                "remove_layer": {"op": "remove_layer", "layer": 0},
                "set_visibility": {"op": "set_visibility", "layer": 0, "visible": true},
                "add_layer": {"op": "add_layer"},
                "duplicate_layer": {"op": "duplicate_layer", "layer": 0},
                "clear_layer": {"op": "clear_layer", "layer": 0},
                "clear_region": {"op": "clear_region", "layer": 0, "x": 0, "y": 0, "width": 1, "height": 1},
                "copy_region": {"op": "copy_region", "layer": 0, "x": 0, "y": 0, "width": 1, "height": 1, "to_layer": 0, "to_x": 1, "to_y": 1},
                "replace_color": {"op": "replace_color", "layer": 0, "from": [0, 0, 0, 0], "to": [255, 0, 0, 255]},
                "flip_layer": {"op": "flip_layer", "layer": 0, "axis": "horizontal"},
                "translate_layer": {"op": "translate_layer", "layer": 0, "dx": 1, "dy": 0, "clip": false},
                "resize_canvas": {"op": "resize_canvas", "width": 32, "height": 32, "clip": false},
                "pixel": {"op": "pixel", "layer": 0, "x": 0, "y": 0, "color": [255, 0, 0, 255]},
                "line": {"op": "line", "layer": 0, "from": [0, 0], "to": [1, 1], "color": [255, 0, 0, 255]},
                "brush_line": {"op": "brush_line", "layer": 0, "from": [0, 0], "to": [1, 1], "size": 3, "color": [255, 0, 0, 255]},
                "rectangle": {"op": "rectangle", "layer": 0, "x": 0, "y": 0, "width": 1, "height": 1, "color": [255, 0, 0, 255]},
                "rectangle_outline": {"op": "rectangle_outline", "layer": 0, "x": 0, "y": 0, "width": 1, "height": 1, "color": [255, 0, 0, 255]},
                "ellipse": {"op": "ellipse", "layer": 0, "x": 0, "y": 0, "width": 1, "height": 1, "filled": true, "color": [255, 0, 0, 255]},
                "fill": {"op": "fill", "layer": 0, "x": 0, "y": 0, "color": [255, 0, 0, 255]}
            },
            "experimental_prompt_openrouter": {"key_input": "stdin only; pipe or redirected input, not a terminal", "prompt_input": "UTF-8 file", "max_prompt_bytes": provider::MAX_PROMPT_BYTES, "visible_composite_opt_in": "--include-composite", "max_composite_pixels": provider::MAX_COMPOSITE_CONTEXT_PIXELS, "model_compatibility": "unverified", "network": "fixed OpenRouter HTTPS endpoint", "cost": "may incur charges; no spend cap", "execution": "data-only typed edits on new layers; no generated code"},
            "autonomy_intake": {"operation": "inspect-autonomy-request", "version": perveance::supervisor::INTAKE_VERSION, "default_maximum": perveance::supervisor::IntakePolicy::default().maximum, "task_created": false, "budgets_enforced": false, "autonomous_execution": false},
            "fixed_worker_diagnostic": {"operation": "verify-fixed-worker", "platform": "linux", "fixed_code_only": true, "autonomous_execution": false, "security_boundary": false, "resource_enforcement": "experimental output cap and best-effort wall timeout only"},
            "project_staging": {"operation": "verify-project-staging", "platform": "linux", "contents": "one validated project snapshot", "persistent": false, "autonomous_execution": false, "security_boundary": false},
            "region_editing": {"source": "raw layer RGBA, including hidden pixels", "paste": "replace bytes, not alpha blending", "bounds": "source and destination must fit; no clipping", "overlap": "source snapshot before writing"},
            "palette_format": {"version": perveance::PALETTE_VERSION, "slots": perveance::PALETTE_SLOTS, "max_bytes": perveance::MAX_PALETTE_BYTES, "color": "straight-alpha RGBA bytes"},
            "proposal_version": perveance::PROPOSAL_VERSION,
            "proposal_example": {"version": 1, "base_revision": "<revision from inspect>", "operations": [{"op": "pixel", "layer": 0, "x": 0, "y": 0, "color": [255, 0, 0, 255]}]},
            "proposal_semantics": "Data-only typed edits bound to a project snapshot fingerprint; input is loaded once and output is create-new. This does not lock files or detect unsaved edits in another process.",
            "batch_semantics": "Layer indices are bottom-to-top; duplicates are inserted immediately above their source. Color replacement matches exact raw RGBA across the selected layer, including hidden/transparent data. Flips reflect the full layer across canvas bounds. Translations shift raw pixels; canvas resize is top-left anchored without resampling. Both require explicit clip=true to discard nonzero RGBA outside the canvas. Brush strokes replace raw pixels, use square stamps clipped to canvas, and bias even sizes toward the top left. RGBA channels are straight-alpha bytes. Shape dimensions are positive and must fit the canvas; ellipse outlines use four-connected edges. Operations apply in order to a clone; on failure no output is created.",
            "limitations": ["create-new outputs only", "PNG import requires static images and converts to 8-bit RGBA without color management", "no vector support", "experimental OpenRouter integration is unverified against a live provider"]})),
        [command, width, height, output] if command == "new" => {
            let project = Project::new(number(width)?, number(height)?)?;
            project.save_new(Path::new(output))?;
            Ok(json!({"ok": true, "output": output}))
        }
        [command, png_path, output] if command == "new-from-png" => {
            let project = Project::from_png(Path::new(png_path))?;
            project.save_new(Path::new(output))?;
            Ok(json!({"ok": true, "output": output, "width": project.width, "height": project.height, "layer": 0}))
        }
        [command, input, png_path, output] if command == "import-png" => {
            let mut project = Project::load(Path::new(input))?;
            project.import_png_layer(Path::new(png_path))?;
            project.save_new(Path::new(output))?;
            Ok(json!({"ok": true, "output": output, "layer": project.layers.len() - 1}))
        }
        [command, input, request] if command == "inspect-autonomy-request" => {
            inspect_autonomy_request(input, request, None)
        }
        [command, input, request, policy] if command == "inspect-autonomy-request" => {
            inspect_autonomy_request(input, request, Some(policy))
        }
        [command, input, parent] if command == "verify-fixed-worker" => {
            let project = Project::load(Path::new(input))?;
            let revision = project.revision()?;
            let outcome = perveance::fixed_worker::run_fixed_worker(&project, Path::new(parent), perveance::fixed_worker::FixedMode::Inspect,
                std::time::Duration::from_secs(10), &std::sync::atomic::AtomicBool::new(false))?;
            if outcome != perveance::fixed_worker::FixedOutcome::Completed { return Err(format!("fixed worker diagnostic: {outcome:?}")); }
            Ok(json!({"ok": true, "revision": revision, "fixed_code_only": true, "autonomous_execution": false, "security_boundary": false}))
        }
        [command, input, parent] if command == "verify-project-staging" => {
            let project = Project::load(Path::new(input))?;
            let staged = perveance::workspace::stage_project(&project, Path::new(parent))?;
            let revision = staged.revision().to_owned();
            staged.close()?;
            Ok(json!({"ok": true, "revision": revision, "staged_files": 1, "persistent": false,
                "autonomous_execution": false, "security_boundary": false}))
        }
        [command, input] if command == "inspect-palette" => {
            let palette = Palette::load(Path::new(input))?;
            Ok(json!({"ok": true, "version": palette.version, "colors": palette.colors}))
        }
        [command, input] if command == "inspect" => {
            let project = Project::load(Path::new(input))?;
            Ok(json!({"ok": true, "version": project.version, "revision": project.revision()?, "width": project.width, "height": project.height,
                "layers": project.layers.iter().map(|layer| json!({"name": layer.name, "visible": layer.visible})).collect::<Vec<_>>() }))
        }
        [command, input, x, y, source] if command == "sample" && source == "composite" => {
            let project = Project::load(Path::new(input))?;
            let (x, y) = (number(x)?, number(y)?);
            let color = project.sample_composite(x, y)?;
            Ok(json!({"ok": true, "x": x, "y": y, "source": "composite", "color": color}))
        }
        [command, input, x, y, source, layer] if command == "sample" && source == "layer" => {
            let project = Project::load(Path::new(input))?;
            let (x, y) = (number(x)?, number(y)?);
            let layer: usize = layer.parse().map_err(|_| "invalid layer index".to_string())?;
            let color = project.sample_layer(layer, x, y)?;
            Ok(json!({"ok": true, "x": x, "y": y, "source": "layer", "layer": layer, "color": color}))
        }
        [command, input, x, y, width, height, source] if command == "read-region" && source == "composite" => {
            let project = Project::load(Path::new(input))?;
            read_region(&project, number(x)?, number(y)?, number(width)?, number(height)?, None)
        }
        [command, input, x, y, width, height, source, layer] if command == "read-region" && source == "layer" => {
            let project = Project::load(Path::new(input))?;
            let index: usize = layer.parse().map_err(|_| "invalid layer index".to_string())?;
            read_region(&project, number(x)?, number(y)?, number(width)?, number(height)?, Some(index))
        }
        [command, input, output, model, prompt_path] if command == "prompt-openrouter" => {
            prompt_openrouter(input, output, model, prompt_path, false)
        }
        [command, input, output, model, prompt_path, flag] if command == "prompt-openrouter" && flag == "--include-composite" => {
            prompt_openrouter(input, output, model, prompt_path, true)
        }
        [command, input, output, layer, x, y, r, g, b, a] if command == "paint" => {
            let mut project = Project::load(Path::new(input))?;
            let layer: usize = layer.parse().map_err(|_| "invalid layer index".to_string())?;
            project.set_pixel(layer, number(x)?, number(y)?, [byte(r)?, byte(g)?, byte(b)?, byte(a)?])?;
            project.save_new(Path::new(output))?;
            Ok(json!({"ok": true, "output": output}))
        }
        [command, input, output, ops_path] if command == "apply" => {
            const MAX_BATCH_BYTES: u64 = 256 * 1024;
            let file = File::open(ops_path).map_err(|e| format!("open operations: {e}"))?;
            let mut bytes = Vec::new();
            file.take(MAX_BATCH_BYTES + 1).read_to_end(&mut bytes).map_err(|e| format!("read operations: {e}"))?;
            if bytes.len() as u64 > MAX_BATCH_BYTES { return Err("operations file exceeds size limit".into()); }
            let ops: Vec<Operation> = serde_json::from_slice(&bytes).map_err(|e| format!("parse operations: {e}"))?;
            let mut project = Project::load(Path::new(input))?;
            project.apply_operations(&ops)?;
            project.save_new(Path::new(output))?;
            Ok(json!({"ok": true, "output": output, "applied": ops.len()}))
        }
        [command, input, output, proposal_path] if command == "apply-proposal" => {
            let proposal = EditProposal::load(Path::new(proposal_path))?;
            let mut project = Project::load(Path::new(input))?;
            let base_revision = project.revision()?;
            project.apply_proposal(&proposal)?;
            let revision = project.revision()?;
            project.save_new(Path::new(output))?;
            Ok(json!({"ok": true, "output": output, "applied": proposal.operations.len(), "base_revision": base_revision, "revision": revision}))
        }
        [command, input, output, scale] if command == "export-png-scaled" => {
            let project = Project::load(Path::new(input))?;
            let scale = number(scale)?;
            project.export_png_scaled_new(Path::new(output), scale)?;
            Ok(json!({"ok": true, "output": output, "width": project.width * scale, "height": project.height * scale, "scale": scale}))
        }
        [command, input, output] if command == "export-png" => {
            let project = Project::load(Path::new(input))?;
            project.export_png_new(Path::new(output))?;
            Ok(json!({"ok": true, "output": output, "width": project.width, "height": project.height}))
        }
        _ => Err("usage: capabilities | new WIDTH HEIGHT OUTPUT | new-from-png PNG OUTPUT | import-png INPUT PNG OUTPUT | inspect INPUT | inspect-palette INPUT | verify-project-staging INPUT TRUSTED_PARENT | verify-fixed-worker INPUT TRUSTED_PARENT | inspect-autonomy-request INPUT REQUEST_JSON [TRUSTED_POLICY_JSON] | sample INPUT X Y composite | sample INPUT X Y layer LAYER | read-region INPUT X Y WIDTH HEIGHT composite | read-region INPUT X Y WIDTH HEIGHT layer LAYER | prompt-openrouter INPUT OUTPUT MODEL PROMPT_FILE [--include-composite] < KEY_STDIN | paint INPUT OUTPUT LAYER X Y R G B A | apply INPUT OUTPUT OPERATIONS_JSON | apply-proposal INPUT OUTPUT PROPOSAL_JSON | export-png INPUT OUTPUT | export-png-scaled INPUT OUTPUT SCALE".into()),
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(value) => println!("{value}"),
        Err(error) => { eprintln!("{}", json!({"ok": false, "error": error})); process::exit(1); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn secret_input_is_bounded_and_never_in_errors() {
        assert_eq!(read_key(&b"test-key\n"[..]).unwrap(), "test-key");
        assert_eq!(read_key(&b"test-key\r\n"[..]).unwrap(), "test-key");
        assert_eq!(read_key(&b"test-key"[..]).unwrap(), "test-key");
        assert_eq!(read_key(vec![b'x'; provider::MAX_API_KEY_BYTES].as_slice()).unwrap().len(), provider::MAX_API_KEY_BYTES);
        assert!(read_key(vec![b'x'; provider::MAX_API_KEY_BYTES + 1].as_slice()).is_err());
        assert!(read_key(&[0xff][..]).is_err());
        let key = read_key(&b"secret\nextra\n"[..]).unwrap();
        let error = provider::validate_request("test/model", &key, "draw").unwrap_err();
        assert!(!error.contains("secret"));
    }

    #[test]
    fn opt_in_context_is_bounded_before_request() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.json");
        let output = dir.path().join("output.json");
        let p = Project::new(33, 32).unwrap();
        p.save_new(&input).unwrap();
        let called = Cell::new(false);
        let result = prompt_to_project(&input, &output, "test/model", "draw", "fake-key", true, |_, _, _, _| {
            called.set(true); unreachable!()
        });
        assert!(result.unwrap_err().contains("1024"));
        assert!(!called.get());
        assert!(!output.exists());
        let revision = p.revision().unwrap();
        let result = prompt_to_project(&input, &output, "test/model", "draw", "fake-key", false, |_, _, _, _| {
            Ok(EditProposal { version: perveance::PROPOSAL_VERSION, base_revision: revision, operations: vec![Operation::AddLayer] })
        }).unwrap();
        assert_eq!(result["shared_composite"], false);
    }

    #[test]
    fn opt_in_context_flag_is_reported_on_success() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.json");
        let output = dir.path().join("output.json");
        let p = Project::new(2, 2).unwrap();
        p.save_new(&input).unwrap();
        let revision = p.revision().unwrap();
        let result = prompt_to_project(&input, &output, "test/model", "draw", "fake-key", true, |_, _, _, _| {
            Ok(EditProposal { version: perveance::PROPOSAL_VERSION, base_revision: revision, operations: vec![Operation::AddLayer] })
        }).unwrap();
        assert_eq!(result["shared_composite"], true);
    }

    #[test]
    fn prompt_pipeline_applies_only_new_layers_and_preserves_input() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.json");
        let output = dir.path().join("output.json");
        let rejected = dir.path().join("rejected.json");
        let mut p = Project::new(2, 2).unwrap();
        p.set_pixel(0, 0, 0, [12, 34, 56, 255]).unwrap();
        p.save_new(&input).unwrap();
        let original = std::fs::read(&input).unwrap();
        let revision = p.revision().unwrap();
        let value = prompt_to_project(&input, &output, "test/model", "draw a star", "fake-key", false, |_, model, key, prompt| {
            assert_eq!((model, key, prompt), ("test/model", "fake-key", "draw a star"));
            Ok(EditProposal { version: perveance::PROPOSAL_VERSION, base_revision: revision.clone(), operations: vec![
                Operation::AddLayer,
                Operation::Pixel { layer: 1, x: 1, y: 1, color: [1, 2, 3, 255] },
            ] })
        }).unwrap();
        assert_eq!(value["applied"], 2);
        assert!(!value.to_string().contains("fake-key"));
        assert!(!value.to_string().contains("draw a star"));
        assert_eq!(std::fs::read(&input).unwrap(), original);
        let result = Project::load(&output).unwrap();
        assert_eq!(result.layers[0], p.layers[0]);
        assert_eq!(result.sample_layer(1, 1, 1).unwrap(), [1, 2, 3, 255]);
        let called = Cell::new(false);
        assert!(prompt_to_project(&input, &output, "test/model", "draw", "fake-key", false, |_, _, _, _| {
            called.set(true); unreachable!()
        }).is_err());
        assert!(!called.get());
        for ops in [
            vec![Operation::Pixel { layer: 0, x: 0, y: 0, color: [0; 4] }],
            vec![Operation::AddLayer, Operation::ResizeCanvas { width: 1, height: 1, clip: true }],
            vec![Operation::AddLayer, Operation::Pixel { layer: 1, x: 99, y: 0, color: [1; 4] }],
        ] {
            let failure = prompt_to_project(&input, &rejected, "test/model", "draw", "fake-key", false, |_, _, _, _| {
                Ok(EditProposal { version: perveance::PROPOSAL_VERSION, base_revision: revision.clone(), operations: ops })
            });
            assert!(failure.is_err());
            assert!(!rejected.exists());
        }
        let stale = prompt_to_project(&input, &rejected, "test/model", "draw", "fake-key", false, |_, _, _, _| {
            Ok(EditProposal { version: perveance::PROPOSAL_VERSION, base_revision: format!("sha256-json-v1:{}", "0".repeat(64)), operations: vec![Operation::AddLayer] })
        });
        assert!(stale.is_err());
        assert!(!rejected.exists());
        assert_eq!(std::fs::read(&input).unwrap(), original);
    }
}

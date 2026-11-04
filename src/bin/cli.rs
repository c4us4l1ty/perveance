use perveance::{Operation, Project};
use serde_json::json;
use std::{env, fs::File, io::Read, path::Path, process};

fn number(s: &str) -> Result<u32, String> { s.parse().map_err(|_| format!("invalid number: {s}")) }
fn byte(s: &str) -> Result<u8, String> { s.parse().map_err(|_| format!("invalid color channel: {s}")) }

fn run(args: &[String]) -> Result<serde_json::Value, String> {
    match args {
        [command] if command == "capabilities" => Ok(json!({"api_version": 1, "prototype": true, "autonomous_execution": false,
            "operations": ["new", "inspect", "paint", "apply", "export-png"],
            "batch_operations": ["add_layer", "set_visibility", "rename_layer", "move_layer", "remove_layer", "pixel", "line", "rectangle", "fill"],
            "limits": {"canvas_dimension": 256, "layers": 32, "batch_operations": 1024, "batch_bytes": 262144},
            "limitations": ["create-new outputs only", "no vector support", "no AI"]})),
        [command, width, height, output] if command == "new" => {
            let project = Project::new(number(width)?, number(height)?)?;
            project.save_new(Path::new(output))?;
            Ok(json!({"ok": true, "output": output}))
        }
        [command, input] if command == "inspect" => {
            let project = Project::load(Path::new(input))?;
            Ok(json!({"ok": true, "version": project.version, "width": project.width, "height": project.height,
                "layers": project.layers.iter().map(|layer| json!({"name": layer.name, "visible": layer.visible})).collect::<Vec<_>>() }))
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
        [command, input, output] if command == "export-png" => {
            let project = Project::load(Path::new(input))?;
            project.export_png_new(Path::new(output))?;
            Ok(json!({"ok": true, "output": output, "width": project.width, "height": project.height}))
        }
        _ => Err("usage: capabilities | new WIDTH HEIGHT OUTPUT | inspect INPUT | paint INPUT OUTPUT LAYER X Y R G B A | apply INPUT OUTPUT OPERATIONS_JSON | export-png INPUT OUTPUT".into()),
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(value) => println!("{value}"),
        Err(error) => { eprintln!("{}", json!({"ok": false, "error": error})); process::exit(1); }
    }
}

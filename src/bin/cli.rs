use perveance::{Operation, Project};
use serde_json::json;
use std::{env, fs::File, io::Read, path::Path, process};

fn number(s: &str) -> Result<u32, String> { s.parse().map_err(|_| format!("invalid number: {s}")) }
fn byte(s: &str) -> Result<u8, String> { s.parse().map_err(|_| format!("invalid color channel: {s}")) }

fn run(args: &[String]) -> Result<serde_json::Value, String> {
    match args {
        [command] if command == "capabilities" => Ok(json!({"api_version": 1, "prototype": true, "autonomous_execution": false,
            "operations": ["new", "new-from-png", "inspect", "sample", "paint", "apply", "import-png", "export-png"],
            "batch_operations": ["add_layer", "set_visibility", "rename_layer", "move_layer", "remove_layer", "pixel", "line", "rectangle", "rectangle_outline", "ellipse", "fill"],
            "limits": {"canvas_dimension": 256, "layers": 32, "batch_operations": 1024, "batch_bytes": 262144, "png_bytes": perveance::MAX_PNG_BYTES},
            "batch_examples": {
                "rename_layer": {"op": "rename_layer", "layer": 0, "name": "Ink"},
                "move_layer": {"op": "move_layer", "from": 0, "to": 1},
                "remove_layer": {"op": "remove_layer", "layer": 0},
                "set_visibility": {"op": "set_visibility", "layer": 0, "visible": true},
                "add_layer": {"op": "add_layer"},
                "pixel": {"op": "pixel", "layer": 0, "x": 0, "y": 0, "color": [255, 0, 0, 255]},
                "line": {"op": "line", "layer": 0, "from": [0, 0], "to": [1, 1], "color": [255, 0, 0, 255]},
                "rectangle": {"op": "rectangle", "layer": 0, "x": 0, "y": 0, "width": 1, "height": 1, "color": [255, 0, 0, 255]},
                "rectangle_outline": {"op": "rectangle_outline", "layer": 0, "x": 0, "y": 0, "width": 1, "height": 1, "color": [255, 0, 0, 255]},
                "ellipse": {"op": "ellipse", "layer": 0, "x": 0, "y": 0, "width": 1, "height": 1, "filled": true, "color": [255, 0, 0, 255]},
                "fill": {"op": "fill", "layer": 0, "x": 0, "y": 0, "color": [255, 0, 0, 255]}
            },
            "batch_semantics": "Layer indices are bottom-to-top. RGBA channels are straight-alpha bytes. Shape dimensions are positive and must fit the canvas; ellipse outlines use four-connected edges. Operations apply in order to a clone; on failure no output is created.",
            "limitations": ["create-new outputs only", "PNG import requires static images and converts to 8-bit RGBA without color management", "no vector support", "no AI"]})),
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
        [command, input] if command == "inspect" => {
            let project = Project::load(Path::new(input))?;
            Ok(json!({"ok": true, "version": project.version, "width": project.width, "height": project.height,
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
        _ => Err("usage: capabilities | new WIDTH HEIGHT OUTPUT | new-from-png PNG OUTPUT | import-png INPUT PNG OUTPUT | inspect INPUT | sample INPUT X Y composite | sample INPUT X Y layer LAYER | paint INPUT OUTPUT LAYER X Y R G B A | apply INPUT OUTPUT OPERATIONS_JSON | export-png INPUT OUTPUT".into()),
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(value) => println!("{value}"),
        Err(error) => { eprintln!("{}", json!({"ok": false, "error": error})); process::exit(1); }
    }
}

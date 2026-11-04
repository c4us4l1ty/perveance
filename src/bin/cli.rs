use perveance::Project;
use serde_json::json;
use std::{env, path::Path, process};

fn number(s: &str) -> Result<u32, String> { s.parse().map_err(|_| format!("invalid number: {s}")) }
fn byte(s: &str) -> Result<u8, String> { s.parse().map_err(|_| format!("invalid color channel: {s}")) }

fn run(args: &[String]) -> Result<serde_json::Value, String> {
    match args {
        [command] if command == "capabilities" => Ok(json!({"api_version": 1, "prototype": true, "autonomous_execution": false,
            "operations": ["new", "inspect", "paint", "export-png"], "limitations": ["create-new outputs only", "no vector support", "no AI"]})),
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
        [command, input, output] if command == "export-png" => {
            let project = Project::load(Path::new(input))?;
            project.export_png_new(Path::new(output))?;
            Ok(json!({"ok": true, "output": output, "width": project.width, "height": project.height}))
        }
        _ => Err("usage: capabilities | new WIDTH HEIGHT OUTPUT | inspect INPUT | paint INPUT OUTPUT LAYER X Y R G B A | export-png INPUT OUTPUT".into()),
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(value) => println!("{value}"),
        Err(error) => { eprintln!("{}", json!({"ok": false, "error": error})); process::exit(1); }
    }
}

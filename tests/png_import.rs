use perveance::Project;
use png::{BitDepth, ColorType};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);
struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("perveance-png-test-{}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

fn png_file(path: &Path, width: u32, height: u32, color: ColorType, depth: BitDepth, data: &[u8], indexed: bool, animated: bool) {
    let mut encoder = png::Encoder::new(fs::File::create(path).unwrap(), width, height);
    encoder.set_color(color);
    encoder.set_depth(depth);
    if indexed { encoder.set_palette(vec![10, 20, 30, 100, 110, 120]); encoder.set_trns(vec![0, 128]); }
    if animated { encoder.set_animated(1, 0).unwrap(); }
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(data).unwrap();
    writer.finish().unwrap();
}

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_perveance-cli")).args(args).output().unwrap()
}

#[test]
fn import_converts_color_types_to_straight_rgba() {
    let dir = TempDir::new();
    let path = dir.0.join("input.png");
    png_file(&path, 2, 1, ColorType::Rgba, BitDepth::Eight, &[1, 2, 3, 4, 5, 6, 7, 8], false, false);
    let p = Project::from_png(&path).unwrap();
    assert_eq!((p.width, p.height), (2, 1));
    assert_eq!(p.layers[0].pixels, [[1, 2, 3, 4], [5, 6, 7, 8]]);

    png_file(&path, 1, 1, ColorType::Rgb, BitDepth::Eight, &[10, 20, 30], false, false);
    assert_eq!(Project::from_png(&path).unwrap().layers[0].pixels, [[10, 20, 30, 255]]);
    png_file(&path, 1, 1, ColorType::Grayscale, BitDepth::Eight, &[37], false, false);
    assert_eq!(Project::from_png(&path).unwrap().layers[0].pixels, [[37, 37, 37, 255]]);
    png_file(&path, 1, 1, ColorType::GrayscaleAlpha, BitDepth::Eight, &[88, 47], false, false);
    assert_eq!(Project::from_png(&path).unwrap().layers[0].pixels, [[88, 88, 88, 47]]);
    png_file(&path, 2, 1, ColorType::Indexed, BitDepth::Two, &[0b0001_0000], true, false);
    assert_eq!(Project::from_png(&path).unwrap().layers[0].pixels, [[10, 20, 30, 0], [100, 110, 120, 128]]);
    png_file(&path, 1, 1, ColorType::Rgb, BitDepth::Sixteen, &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc], false, false);
    assert_eq!(Project::from_png(&path).unwrap().layers[0].pixels, [[0x12, 0x56, 0x9a, 255]]);
}

#[test]
fn rejects_invalid_dimensions_animation_corruption_and_oversized_input() {
    let dir = TempDir::new();
    let path = dir.0.join("input.png");
    png_file(&path, 257, 1, ColorType::Rgba, BitDepth::Eight, &vec![0; 257 * 4], false, false);
    assert!(Project::from_png(&path).unwrap_err().contains("dimensions"));
    png_file(&path, 1, 1, ColorType::Rgba, BitDepth::Eight, &[1, 2, 3, 255], false, true);
    assert!(Project::from_png(&path).unwrap_err().contains("animated"));
    fs::write(&path, b"not a png").unwrap();
    assert!(Project::from_png(&path).is_err());
    png_file(&path, 1, 1, ColorType::Rgba, BitDepth::Eight, &[1, 2, 3, 255], false, false);
    let mut data = fs::read(&path).unwrap();
    data.truncate(data.len() / 2);
    fs::write(&path, data).unwrap();
    assert!(Project::from_png(&path).is_err());
    fs::write(&path, vec![0; perveance::MAX_PNG_BYTES as usize + 1]).unwrap();
    assert!(Project::from_png(&path).unwrap_err().contains("size limit"));
}

#[test]
fn layer_import_is_transactional_and_requires_matching_canvas() {
    let dir = TempDir::new();
    let path = dir.0.join("input.png");
    png_file(&path, 2, 1, ColorType::Rgba, BitDepth::Eight, &[1, 2, 3, 4, 5, 6, 7, 8], false, false);
    let mut p = Project::new(1, 1).unwrap();
    let before = p.clone();
    assert!(p.import_png_layer(&path).is_err());
    assert_eq!(p, before);
    let mut p = Project::new(2, 1).unwrap();
    p.import_png_layer(&path).unwrap();
    assert_eq!(p.layers.len(), 2);
    assert_eq!(p.layers[1].name, "Imported PNG");
    assert_eq!(p.layers[1].pixels[1], [5, 6, 7, 8]);
    while p.layers.len() < 32 { p.add_layer().unwrap(); }
    let before = p.clone();
    assert!(p.import_png_layer(&path).is_err());
    assert_eq!(p, before);
}

#[test]
fn cli_import_creates_editable_projects_and_rejects_bad_inputs() {
    let dir = TempDir::new();
    let png = dir.0.join("image.png");
    let new_project = dir.0.join("from-image.json");
    let layered = dir.0.join("layered.json");
    let bad_output = dir.0.join("bad.json");
    png_file(&png, 2, 1, ColorType::Rgba, BitDepth::Eight, &[11, 22, 33, 255, 44, 55, 66, 128], false, false);
    let result = cli(&["new-from-png", png.to_str().unwrap(), new_project.to_str().unwrap()]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let result = cli(&["import-png", new_project.to_str().unwrap(), png.to_str().unwrap(), layered.to_str().unwrap()]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let p = Project::load(&layered).unwrap();
    assert_eq!(p.layers.len(), 2);
    assert_eq!(p.layers[1].pixels[1], [44, 55, 66, 128]);
    let previous = fs::read(&layered).unwrap();
    assert!(!cli(&["import-png", new_project.to_str().unwrap(), png.to_str().unwrap(), layered.to_str().unwrap()]).status.success());
    assert_eq!(fs::read(&layered).unwrap(), previous);
    fs::write(&png, b"broken").unwrap();
    assert!(!cli(&["new-from-png", png.to_str().unwrap(), bad_output.to_str().unwrap()]).status.success());
    assert!(!bad_output.exists());
}

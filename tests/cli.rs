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

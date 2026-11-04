use perveance::Project;
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

#[test]
fn maximum_valid_project_is_saveable_and_reopens() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("large.json");
    let mut project = Project::new(256, 256).unwrap();
    while project.layers.len() < 32 { project.add_layer().unwrap(); }
    for layer in &mut project.layers {
        layer.name = "\0".repeat(256);
        layer.pixels.fill([255, 255, 255, 255]);
    }
    project.validate().unwrap();
    project.save_new(&output).unwrap();
    assert!(fs::metadata(&output).unwrap().len() > 16 * 1024 * 1024);
    assert_eq!(Project::load(&output).unwrap(), project);
}

#[test]
fn simultaneous_create_new_saves_do_not_overwrite_or_leave_temporary_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = Arc::new(dir.path().join("shared.json"));
    let start = Arc::new(Barrier::new(8));
    let mut threads = Vec::new();
    for color in 0..8u8 {
        let path = Arc::clone(&path);
        let start = Arc::clone(&start);
        threads.push(thread::spawn(move || {
            let mut p = Project::new(1, 1).unwrap();
            p.set_pixel(0, 0, 0, [color, 0, 0, 255]).unwrap();
            start.wait();
            p.save_new(&path).is_ok()
        }));
    }
    let wins = threads.into_iter().map(|thread| thread.join().unwrap()).filter(|win| *win).count();
    assert_eq!(wins, 1);
    let result = Project::load(&path).unwrap();
    assert_eq!(result.layers[0].pixels[0][3], 255);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn new_file_writes_refuse_existing_files_and_missing_parents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("occupied.json");
    fs::write(&path, b"keep me").unwrap();
    let project = Project::new(1, 1).unwrap();
    assert!(project.save_new(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"keep me");
    assert!(project.save_new(&dir.path().join("missing/output.json")).is_err());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

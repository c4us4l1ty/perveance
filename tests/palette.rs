use perveance::{Palette, MAX_PALETTE_BYTES, PALETTE_SLOTS};
use std::fs;

#[test]
fn palette_file_round_trips_all_channels_and_refuses_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("palette.json");
    let mut colors = [[0; 4]; PALETTE_SLOTS];
    colors[0] = [1, 2, 3, 0];
    colors[15] = [255, 200, 100, 128];
    let palette = Palette::new(colors);
    palette.save_new(&path).unwrap();
    assert_eq!(Palette::load(&path).unwrap(), palette);
    let before = fs::read(&path).unwrap();
    assert!(Palette::new([[255; 4]; PALETTE_SLOTS]).save_new(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(palette.save_new(&dir.path().join("missing/file.json")).is_err());
    #[cfg(unix)] {
        use std::os::unix::fs::symlink;
        let link = dir.path().join("link.json");
        symlink(&path, &link).unwrap();
        assert!(palette.save_new(&link).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::remove_file(link).unwrap();
    }
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn palette_load_strictly_checks_version_shape_channels_and_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("palette.json");
    let palette = Palette::new([[1, 2, 3, 4]; PALETTE_SLOTS]);
    let valid = serde_json::to_value(&palette).unwrap();
    for invalid in [
        serde_json::json!({"version": 2, "colors": valid["colors"]}),
        serde_json::json!({"version": 1, "colors": valid["colors"], "extra": true}),
        serde_json::json!({"version": 1, "colors": [[0,0,0,0]]}),
        serde_json::json!({"version": 1, "colors": vec![vec![0,0,0,256]; PALETTE_SLOTS]}),
        serde_json::json!({"version": 1, "colors": vec![vec![0,0,0]; PALETTE_SLOTS]}),
    ] {
        fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(Palette::load(&path).is_err(), "accepted {invalid}");
    }
    fs::write(&path, vec![b' '; MAX_PALETTE_BYTES as usize + 1]).unwrap();
    assert!(Palette::load(&path).unwrap_err().contains("size limit"));
    let mut unsupported = palette;
    unsupported.version = 2;
    let output = dir.path().join("no-output.json");
    assert!(unsupported.save_new(&output).is_err());
    assert!(!output.exists());
}

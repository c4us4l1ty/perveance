use perveance::{Project, MAX_EXPORT_SCALE};
use std::{fs, path::Path};

fn decode(path: &Path) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(fs::File::open(path).unwrap()).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    (info.width, info.height, pixels[..info.buffer_size()].to_vec())
}

#[test]
fn integer_export_repeats_composite_pixels_without_mutating_project() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = Project::new(3, 2).unwrap();
    p.set_pixel(0, 0, 0, [120, 30, 40, 255]).unwrap();
    p.set_pixel(0, 1, 0, [10, 20, 30, 0]).unwrap();
    p.set_pixel(0, 2, 1, [15, 60, 90, 100]).unwrap();
    p.add_layer().unwrap();
    p.set_pixel(1, 0, 0, [10, 240, 30, 128]).unwrap();
    p.add_layer().unwrap();
    p.set_pixel(2, 1, 1, [200, 200, 200, 255]).unwrap();
    p.layers[2].visible = false;
    let before = p.clone();
    let revision = p.revision().unwrap();
    let expected = p.rgba().unwrap();
    for scale in [1, 2, 3, MAX_EXPORT_SCALE] {
        let path = dir.path().join(format!("scale-{scale}.png"));
        p.export_png_scaled_new(&path, scale).unwrap();
        let (width, height, pixels) = decode(&path);
        assert_eq!((width, height), (3 * scale, 2 * scale));
        for y in 0..height {
            for x in 0..width {
                let dest = ((y * width + x) * 4) as usize;
                let src = (((y / scale) * 3 + (x / scale)) * 4) as usize;
                assert_eq!(&pixels[dest..dest + 4], &expected[src..src + 4]);
            }
        }
    }
    let original = dir.path().join("unscaled.png");
    p.export_png_new(&original).unwrap();
    assert_eq!(decode(&original), decode(&dir.path().join("scale-1.png")));
    assert_eq!(p, before);
    assert_eq!(p.revision().unwrap(), revision);
}

#[test]
fn scaled_export_bounds_and_noclobber_are_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("image.png");
    let p = Project::new(1, 1).unwrap();
    for scale in [0, 9, u32::MAX] {
        assert!(p.export_png_scaled_new(&output, scale).is_err());
        assert!(!output.exists());
    }
    fs::write(&output, b"keep original").unwrap();
    assert!(p.export_png_scaled_new(&output, 2).is_err());
    assert_eq!(fs::read(&output).unwrap(), b"keep original");
    let mut bad = p;
    bad.layers[0].pixels.clear();
    let path = dir.path().join("invalid.png");
    assert!(bad.export_png_scaled_new(&path, 2).is_err());
    assert!(!path.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn largest_supported_canvas_can_export_at_maximum_scale() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("large.png");
    let mut p = Project::new(256, 256).unwrap();
    p.set_pixel(0, 255, 255, [90, 100, 110, 255]).unwrap();
    p.export_png_scaled_new(&output, MAX_EXPORT_SCALE).unwrap();
    let (w, h, pixels) = decode(&output);
    assert_eq!((w, h), (2048, 2048));
    assert_eq!(&pixels[pixels.len() - 4..], &[90, 100, 110, 255]);
}

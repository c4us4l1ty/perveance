use perveance::{History, Operation, Project};

#[test]
fn resize_preserves_raw_pixels_and_hidden_layers_without_scaling() {
    let mut p = Project::new(2, 3).unwrap();
    p.set_pixel(0, 1, 1, [10, 20, 30, 255]).unwrap();
    p.add_layer().unwrap();
    p.layers[1].visible = false;
    p.rename_layer(1, "Hidden ink").unwrap();
    p.set_pixel(1, 0, 2, [7, 8, 9, 0]).unwrap();
    p.resize_canvas(4, 4, false).unwrap();
    assert_eq!((p.width, p.height), (4, 4));
    assert_eq!(p.sample_layer(0, 1, 1).unwrap(), [10,20,30,255]);
    assert_eq!(p.sample_layer(1, 0, 2).unwrap(), [7,8,9,0]);
    assert_eq!(p.sample_layer(1, 3, 3).unwrap(), [0; 4]);
    assert_eq!(p.layers[1].name, "Hidden ink");
    assert!(!p.layers[1].visible);
    let enlarged = p.clone();
    assert!(p.resize_canvas(4, 2, false).unwrap_err().contains("clip"));
    assert_eq!(p, enlarged);
    p.resize_canvas(2, 2, true).unwrap();
    assert_eq!(p.sample_layer(0, 1, 1).unwrap(), [10,20,30,255]);
    assert_eq!(p.layers[1].pixels, vec![[0; 4]; 4]);
    assert!(!p.layers[1].visible);
    p.resize_canvas(1, 2, true).unwrap();
    assert_eq!((p.width, p.height), (1,2));
    assert!(p.layers[0].pixels.iter().all(|pixel| *pixel == [0; 4]));
}

#[test]
fn resize_limits_transactional_failure_and_history() {
    let mut p = Project::new(2, 2).unwrap();
    p.set_pixel(0, 1, 1, [0, 0, 0, 1]).unwrap();
    let original = p.clone();
    for (w,h) in [(0, 1), (1,0), (257,1), (1,257), (u32::MAX, u32::MAX)] {
        assert!(p.resize_canvas(w, h, true).is_err());
        assert_eq!(p, original);
    }
    let ops = [Operation::Pixel { layer: 0, x: 0, y: 0, color: [1;4] }, Operation::ResizeCanvas { width: 1, height: 1, clip: false }];
    assert!(p.apply_operations(&ops).is_err());
    assert_eq!(p, original);
    let mut h = History::default();
    assert!(h.apply(&mut p, |p| p.resize_canvas(1, 1, true)).unwrap());
    assert_eq!((p.width, p.height), (1, 1));
    assert!(h.undo(&mut p));
    assert_eq!(p, original);
    assert!(!h.apply(&mut p, |p| p.resize_canvas(2, 2, false)).unwrap());
    assert!(h.can_redo());
    assert!(h.redo(&mut p));
    assert_eq!((p.width, p.height), (1, 1));
    assert!(h.undo(&mut p));
    assert_eq!(p, original);
}

#[test]
fn shrinking_empty_margins_is_allowed_without_clip() {
    let mut p = Project::new(3, 3).unwrap();
    p.set_pixel(0, 0, 0, [1, 2, 3, 4]).unwrap();
    p.resize_canvas(1, 1, false).unwrap();
    assert_eq!(p.layers[0].pixels, vec![[1,2,3,4]]);
    p.resize_canvas(256, 256, false).unwrap();
    assert_eq!(p.sample_layer(0, 0, 0).unwrap(), [1,2,3,4]);
    assert_eq!(p.sample_layer(0, 255, 255).unwrap(), [0;4]);
}

use perveance::{History, Operation, Project};

#[test]
fn translates_raw_hidden_layer_and_refuses_loss_without_mutation() {
    let mut p = Project::new(3, 2).unwrap();
    p.layers[0].visible = false;
    p.set_pixel(0, 0, 0, [1, 2, 3, 0]).unwrap();
    p.set_pixel(0, 1, 0, [4, 5, 6, 255]).unwrap();
    p.translate_layer(0, 1, 1, false).unwrap();
    assert!(!p.layers[0].visible);
    assert_eq!(p.sample_layer(0, 1, 1).unwrap(), [1, 2, 3, 0]);
    assert_eq!(p.sample_layer(0, 2, 1).unwrap(), [4, 5, 6, 255]);
    assert_eq!(p.sample_layer(0, 0, 0).unwrap(), [0; 4]);
    let before = p.clone();
    assert!(p.translate_layer(0, 1, 0, false).unwrap_err().contains("clip"));
    assert_eq!(p, before);
    assert!(p.translate_layer(1, 0, 0, true).is_err());
    for (dx, dy) in [(257, 0), (-257, 0), (i32::MIN, 0), (0, i32::MAX)] {
        assert!(p.translate_layer(0, dx, dy, true).is_err());
        assert_eq!(p, before);
    }
    p.translate_layer(0, 1, 0, true).unwrap();
    assert_eq!(p.sample_layer(0, 2, 1).unwrap(), [1, 2, 3, 0]);
    p.translate_layer(0, -2, -1, false).unwrap();
    assert_eq!(p.sample_layer(0, 0, 0).unwrap(), [1, 2, 3, 0]);
    let before = p.clone();
    assert!(p.translate_layer(0, -1, 0, false).is_err()); // transparent RGB is data too
    assert_eq!(p, before);
    p.translate_layer(0, -256, 0, true).unwrap();
    assert!(p.layers[0].pixels.iter().all(|pixel| *pixel == [0; 4]));
}

#[test]
fn translation_is_transactional_and_undoable_with_noop_preserving_redo() {
    let mut p = Project::new(2, 2).unwrap();
    p.set_pixel(0, 0, 0, [10, 20, 30, 255]).unwrap();
    let original = p.clone();
    let bad = [Operation::Pixel { layer: 0, x: 1, y: 1, color: [1; 4] },
        Operation::TranslateLayer { layer: 0, dx: 1, dy: 0, clip: false }];
    assert!(p.apply_operations(&bad).is_err());
    assert_eq!(p, original);
    let mut history = History::default();
    assert!(history.apply(&mut p, |p| p.translate_layer(0, 1, 0, false)).unwrap());
    let moved = p.clone();
    assert!(history.undo(&mut p));
    assert_eq!(p, original);
    assert!(!history.apply(&mut p, |p| p.translate_layer(0, 0, 0, false)).unwrap());
    assert!(history.can_redo());
    assert!(history.redo(&mut p));
    assert_eq!(p, moved);
    assert!(history.apply(&mut p, |p| p.translate_layer(0, 1, 0, false)).is_err());
    assert_eq!(p, moved);
    assert!(history.undo(&mut p));
    assert_eq!(p, original);
}

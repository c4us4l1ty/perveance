use perveance::{History, Operation, Project};

#[test]
fn replace_color_uses_exact_rgba_in_one_raw_hidden_layer() {
    let mut p = Project::new(3, 2).unwrap();
    p.set_pixel(0, 0, 0, [7, 8, 9, 0]).unwrap();
    p.add_layer().unwrap();
    p.layers[1].visible = false;
    p.rename_layer(1, "Invisible source").unwrap();
    p.set_pixel(1, 0, 0, [7, 8, 9, 0]).unwrap();
    p.set_pixel(1, 1, 0, [7, 8, 9, 0]).unwrap();
    p.set_pixel(1, 2, 0, [7, 8, 9, 1]).unwrap();
    p.set_pixel(1, 0, 1, [7, 8, 10, 0]).unwrap();
    let first_layer = p.layers[0].clone();
    assert_eq!(p.replace_color(1, [7,8,9,0], [100,200,10,128]).unwrap(), 2);
    assert_eq!(p.layers[0], first_layer);
    assert!(!p.layers[1].visible);
    assert_eq!(p.layers[1].name, "Invisible source");
    assert_eq!(p.sample_layer(1, 0, 0).unwrap(), [100,200,10,128]);
    assert_eq!(p.sample_layer(1, 1, 0).unwrap(), [100,200,10,128]);
    assert_eq!(p.sample_layer(1, 2, 0).unwrap(), [7,8,9,1]);
    assert_eq!(p.sample_layer(1, 0, 1).unwrap(), [7,8,10,0]);
    assert_eq!(p.sample_composite(0,0).unwrap(), [0;4]);
    let before = p.clone();
    assert_eq!(p.replace_color(1, [100,200,10,128], [100,200,10,128]).unwrap(), 0);
    assert_eq!(p.replace_color(1, [1,2,3,4], [0;4]).unwrap(), 0);
    assert_eq!(p, before);
    assert!(p.replace_color(2, [0;4], [1;4]).is_err());
    assert_eq!(p, before);
}

#[test]
fn replacement_can_change_transparency_and_is_undoable_atomically() {
    let mut p = Project::new(2, 2).unwrap();
    let original = p.clone();
    let mut history = History::default();
    assert!(history.apply(&mut p, |p| p.replace_color(0, [0;4], [200,100,50,128]).map(|_| ())).unwrap());
    assert!(p.layers[0].pixels.iter().all(|pixel| *pixel == [200,100,50,128]));
    assert!(history.undo(&mut p));
    assert_eq!(p, original);
    assert!(!history.apply(&mut p, |p| p.replace_color(0, [1;4], [2;4]).map(|_| ())).unwrap());
    assert!(history.can_redo());
    assert!(history.redo(&mut p));
    let recolored = p.clone();
    let batch = [Operation::ReplaceColor { layer: 0, from: [200,100,50,128], to: [0;4] },
        Operation::Pixel { layer: 99, x: 0, y: 0, color: [1;4] }];
    assert!(p.apply_operations(&batch).is_err());
    assert_eq!(p, recolored);
    assert!(history.undo(&mut p));
    assert_eq!(p, original);
}

use perveance::{region::PixelRegion, EditProposal, History, Operation, Project};

fn fixture() -> Project {
    let mut p = Project::new(5, 4).unwrap();
    for y in 0..4 { for x in 0..5 {
        p.set_pixel(0, x, y, [x as u8 + 1, y as u8 + 1, 33, if x % 2 == 0 { 0 } else { 128 }]).unwrap();
    }}
    p.layers[0].visible = false;
    p.add_layer().unwrap();
    p.rectangle(1, 0, 0, 5, 4, [99, 88, 77, 255]).unwrap();
    p
}

#[test]
fn copying_is_an_owned_raw_hidden_layer_snapshot_in_row_order() {
    let mut p = fixture();
    let before = p.clone();
    let mut region = p.copy_region(0, 1, 1, 3, 2).unwrap();
    assert_eq!((region.width, region.height), (3, 2));
    assert_eq!(region.pixels, vec![[2, 2, 33, 128], [3, 2, 33, 0], [4, 2, 33, 128],
                                  [2, 3, 33, 128], [3, 3, 33, 0], [4, 3, 33, 128]]);
    assert_eq!(p, before);
    region.pixels[0] = [9; 4];
    assert_eq!(p, before);
    p.set_pixel(0, 3, 2, [10; 4]).unwrap();
    assert_eq!(region.pixels[5], [4, 3, 33, 128]);
}

#[test]
fn paste_replaces_even_transparent_bytes_without_blending_or_changing_metadata() {
    let mut p = fixture();
    let before = p.clone();
    let region = p.copy_region(0, 1, 1, 3, 2).unwrap();
    p.paste_region(1, 2, 2, &region).unwrap();
    let mut expected = before.clone();
    for y in 0..2 { for x in 0..3 {
        expected.layers[1].pixels[((2 + y) * 5 + 2 + x) as usize] = region.pixels[(y * 3 + x) as usize];
    }}
    assert_eq!(p, expected);
    assert_eq!(p.sample_layer(1, 3, 2).unwrap(), [3, 2, 33, 0]);
    assert_eq!(p.layers[0], before.layers[0]);
    p.clear_region(1, 3, 2, 2, 2).unwrap();
    for y in 2..4 { for x in 3..5 { assert_eq!(p.sample_layer(1, x, y).unwrap(), [0; 4]); }}
}

#[test]
fn overlapping_copies_use_the_original_source_in_every_direction() {
    for (source, destination) in [([0, 0, 4, 3], [1, 1]), ([1, 1, 4, 3], [0, 0]),
        ([0, 0, 4, 4], [1, 0]), ([1, 0, 4, 4], [0, 0]),
        ([0, 0, 5, 3], [0, 1]), ([0, 1, 5, 3], [0, 0]), ([0, 0, 5, 4], [0, 0])] {
        let mut p = fixture();
        let before = p.clone();
        let mut expected = before.clone();
        for y in 0..source[3] { for x in 0..source[2] {
            expected.layers[0].pixels[((destination[1] + y) * 5 + destination[0] + x) as usize] =
                before.layers[0].pixels[((source[1] + y) * 5 + source[0] + x) as usize];
        }}
        p.copy_region_to(0, source, 0, destination).unwrap();
        assert_eq!(p, expected, "{source:?} to {destination:?}");
    }
}

#[test]
fn invalid_regions_and_destinations_never_partially_mutate() {
    let mut p = fixture();
    let before = p.clone();
    for [x, y, width, height] in [[0, 0, 0, 1], [0, 0, 1, 0], [5, 0, 1, 1],
        [0, 4, 1, 1], [4, 3, 2, 1], [0, 0, u32::MAX, 1], [u32::MAX, 0, 1, 1]] {
        assert!(p.copy_region(0, x, y, width, height).is_err());
        assert!(p.clear_region(0, x, y, width, height).is_err());
        assert!(p.copy_region_to(0, [x, y, width, height], 1, [0, 0]).is_err());
        assert_eq!(p, before);
    }
    for region in [PixelRegion { width: 0, height: 1, pixels: vec![] },
        PixelRegion { width: 1, height: 1, pixels: vec![] },
        PixelRegion { width: 1, height: 1, pixels: vec![[0; 4]; 2] },
        PixelRegion { width: u32::MAX, height: u32::MAX, pixels: vec![] },
        PixelRegion { width: 257, height: 1, pixels: vec![[0; 4]; 257] }] {
        assert!(p.paste_region(1, 0, 0, &region).is_err());
        assert_eq!(p, before);
    }
    let region = p.copy_region(0, 0, 0, 2, 2).unwrap();
    for (layer, x, y) in [(1, 4, 0), (1, 0, 3), (1, u32::MAX, 0), (2, 0, 0), (usize::MAX, 0, 0)] {
        assert!(p.paste_region(layer, x, y, &region).is_err());
        assert_eq!(p, before);
    }
    assert!(p.copy_region(2, 0, 0, 1, 1).is_err());
    assert!(p.clear_region(2, 0, 0, 1, 1).is_err());
    assert!(p.copy_region_to(0, [0, 0, 1, 1], 2, [0, 0]).is_err());
    assert_eq!(p, before);
}

#[test]
fn full_size_regions_are_bounded_by_the_canvas_not_the_inspection_limit() {
    let mut p = Project::new(256, 256).unwrap();
    p.set_pixel(0, 255, 255, [9, 8, 7, 0]).unwrap();
    let region = p.copy_region(0, 0, 0, 256, 256).unwrap();
    assert_eq!(region.pixels.len(), 65536);
    p.clear_region(0, 0, 0, 256, 256).unwrap();
    p.paste_region(0, 0, 0, &region).unwrap();
    assert_eq!(p.sample_layer(0, 255, 255).unwrap(), [9, 8, 7, 0]);
}

#[test]
fn region_batches_are_atomic_revision_checked_and_undoable_without_losing_redo() {
    let mut p = fixture();
    let before = p.clone();
    let copy = Operation::CopyRegion { layer: 0, x: 1, y: 1, width: 2, height: 2, to_layer: 1, to_x: 0, to_y: 0 };
    let clear = Operation::ClearRegion { layer: 0, x: 1, y: 1, width: 2, height: 2 };
    let bad = Operation::CopyRegion { layer: 0, x: 0, y: 0, width: 2, height: 2, to_layer: 1, to_x: 4, to_y: 3 };
    assert!(p.apply_operations(&[clear.clone(), bad.clone()]).is_err());
    assert_eq!(p, before);
    let proposal = EditProposal { version: 1, base_revision: p.revision().unwrap(), operations: vec![copy.clone(), clear] };
    let mut history = History::default();
    assert!(history.apply(&mut p, |p| p.apply_proposal(&proposal)).unwrap());
    let after = p.clone();
    assert_eq!(p.copy_region(1, 0, 0, 2, 2).unwrap(), before.copy_region(0, 1, 1, 2, 2).unwrap());
    assert_eq!(p.copy_region(0, 1, 1, 2, 2).unwrap().pixels, vec![[0; 4]; 4]);
    assert!(history.apply(&mut p, |p| p.apply_proposal(&proposal)).is_err());
    assert!(history.undo(&mut p));
    assert_eq!(p, before);
    assert!(history.apply(&mut p, |p| p.apply_operations(&[bad])).is_err());
    assert!(!history.apply(&mut p, |p| p.copy_region_to(0, [0, 0, 5, 4], 0, [0, 0])).unwrap());
    assert!(history.can_redo());
    assert!(history.redo(&mut p));
    assert_eq!(p, after);
}

#[test]
fn provider_cannot_use_region_operations_even_on_new_layers() {
    for operation in [Operation::ClearRegion { layer: 2, x: 0, y: 0, width: 1, height: 1 },
        Operation::CopyRegion { layer: 0, x: 0, y: 0, width: 1, height: 1, to_layer: 2, to_x: 0, to_y: 0 }] {
        let mut p = fixture();
        let before = p.clone();
        let proposal = EditProposal { version: 1, base_revision: p.revision().unwrap(), operations: vec![Operation::AddLayer, operation] };
        assert!(perveance::provider::apply_generated(&mut p, &proposal).is_err());
        assert_eq!(p, before);
    }
}

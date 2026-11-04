use perveance::{Operation, Project, MAX_BRUSH_SIZE};

#[test]
fn square_brush_stamps_exact_raw_rgba_and_clips_at_edges() {
    let mut p = Project::new(5, 5).unwrap();
    p.layers[0].visible = false;
    p.brush_line(0, [0,0], [0,0], 3, [11,22,33,0]).unwrap();
    for y in 0..5 {
        for x in 0..5 {
            let expected = if x < 2 && y < 2 { [11,22,33,0] } else { [0;4] };
            assert_eq!(p.sample_layer(0, x, y).unwrap(), expected);
        }
    }
    assert!(!p.layers[0].visible);
    let mut even = Project::new(5, 5).unwrap();
    even.brush_line(0, [2,2], [2,2], 2, [7,8,9,255]).unwrap();
    for y in 0..5 {
        for x in 0..5 {
            let expected = if (1..=2).contains(&x) && (1..=2).contains(&y) { [7,8,9,255] } else { [0;4] };
            assert_eq!(even.sample_layer(0, x, y).unwrap(), expected);
        }
    }
    even.brush_line(0, [4,4], [4,4], MAX_BRUSH_SIZE, [9,9,9,128]).unwrap();
    assert!(even.layers[0].pixels.iter().all(|p| *p == [9,9,9,128]));
    even.brush_line(0, [4,4], [4,4], MAX_BRUSH_SIZE, [0;4]).unwrap();
    assert!(even.layers[0].pixels.iter().all(|p| *p == [0;4]));
}

#[test]
fn brush_line_connects_points_and_one_pixel_matches_line() {
    for y in 0..5 {
        for x in 0..5 {
            let mut a = Project::new(5, 5).unwrap();
            let mut b = a.clone();
            a.brush_line(0, [0,0], [x,y], 1, [2,3,4,5]).unwrap();
            b.draw_line(0, [0,0], [x,y], [2,3,4,5]).unwrap();
            assert_eq!(a, b);
        }
    }
    let mut p = Project::new(8, 5).unwrap();
    p.brush_line(0, [1,2], [6,2], 3, [20,30,40,255]).unwrap();
    for y in 0..5 {
        for x in 0..8 {
            let expected = if y >= 1 && y <= 3 { [20,30,40,255] } else { [0;4] };
            assert_eq!(p.sample_layer(0, x, y).unwrap(), expected);
        }
    }
}

#[test]
fn invalid_brushes_and_failed_batch_leave_project_unchanged() {
    let mut p = Project::new(2, 2).unwrap();
    let before = p.clone();
    for (layer, from, to, size) in [(0,[0,0],[0,0],0), (0,[0,0],[0,0],17), (1,[0,0],[0,0],1),
        (0,[2,0],[0,0],1), (0,[0,0],[u32::MAX,0],1), (0,[0,0],[0,0],u32::MAX)] {
        assert!(p.brush_line(layer, from, to, size, [1;4]).is_err());
        assert_eq!(p, before);
    }
    let ops = [Operation::Pixel { layer: 0, x: 0, y: 0, color: [1;4] },
        Operation::BrushLine { layer: 0, from: [0,0], to: [1,1], size: 0, color: [2;4] }];
    assert!(p.apply_operations(&ops).is_err());
    assert_eq!(p, before);
}

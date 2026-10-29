use super::*;
#[test]
fn proven_outer_rect_clip_bounds_nested_unsupported_effect_without_ignoring_it() {
    let shared = collector();
    target(&shared);
    shared.borrow_mut().push(
        Affine::translate((100.0, 0.0)),
        &sharp_padding_path(),
        false,
    );
    {
        let _effect = Collector::enter(&shared, 99);
        shared.borrow_mut().unsupported();
        shared
            .borrow_mut()
            .push(Affine::translate((100.0, 0.0)), &sharp_padding_path(), true);
        shared.borrow_mut().draw(None);
        shared
            .borrow_mut()
            .draw(Some(Rect::new(0.0, 0.0, 200.0, 100.0)));
        shared.borrow_mut().pop();
    }
    shared.borrow_mut().pop();
    assert_eq!(
        shared.borrow().finish().unwrap().targets[&1].later_overlap,
        Overlap::Clear
    );
    // After the real outer clip ends, the identical unknown primitive is unsafe again.
    for _ in 0..12 {
        shared.borrow_mut().draw(None);
    }
    assert_eq!(
        shared.borrow().finish().unwrap().targets[&1].later_overlap,
        Overlap::Unknown
    );
    assert_eq!(shared.borrow().diagnostics, 8);
}
#[test]
fn unknown_primitive_uses_actual_disjoint_rectangular_clip_only_as_upper_bound() {
    let shared = collector();
    target(&shared);
    shared.borrow_mut().push(
        Affine::translate((100.0, 0.0)),
        &sharp_padding_path(),
        false,
    );
    shared.borrow_mut().draw(None);
    shared.borrow_mut().pop();
    assert_eq!(
        shared.borrow().finish().unwrap().targets[&1].later_overlap,
        Overlap::Clear
    );
    shared
        .borrow_mut()
        .draw(Some(Rect::new(15.0, 15.0, 20.0, 20.0)));
    shared.borrow_mut().push(
        Affine::translate((100.0, 0.0)),
        &sharp_padding_path(),
        false,
    );
    shared.borrow_mut().draw(None);
    shared.borrow_mut().pop();
    assert_eq!(
        shared.borrow().finish().unwrap().targets[&1].later_overlap,
        Overlap::Overlap
    );
}
#[test]
fn overlapping_clip_and_unmeasured_unclipped_draw_remain_unknown() {
    for clipped in [false, true] {
        let shared = collector();
        target(&shared);
        if clipped {
            shared
                .borrow_mut()
                .push(Affine::IDENTITY, &sharp_padding_path(), false);
        }
        shared.borrow_mut().draw(None);
        if clipped {
            shared.borrow_mut().pop();
        }
        assert_eq!(
            shared.borrow().finish().unwrap().targets[&1].later_overlap,
            Overlap::Unknown
        );
    }
}
#[test]
fn unknown_effect_rounded_and_transformed_clip_do_not_gain_separation() {
    for mode in 0..3 {
        let shared = collector();
        target(&shared);
        {
            let _other = Collector::enter(&shared, 99);
            if mode == 0 {
                shared.borrow_mut().push(
                    Affine::translate((100.0, 0.0)),
                    &kurbo::RoundedRect::new(0.0, 0.0, 80.0, 80.0, 8.0),
                    false,
                );
            } else {
                let transform = if mode == 1 {
                    Affine::translate((100.0, 0.0)) * Affine::rotate(0.2)
                } else {
                    Affine::translate((100.0, 0.0))
                };
                shared
                    .borrow_mut()
                    .push(transform, &sharp_padding_path(), mode == 2);
                if mode == 2 {
                    shared.borrow_mut().unsupported();
                }
            }
            shared.borrow_mut().draw(None);
            shared.borrow_mut().pop();
        }
        assert_eq!(
            shared.borrow().finish().unwrap().targets[&1].later_overlap,
            Overlap::Unknown
        );
    }
}
fn sharp_padding_path() -> kurbo::BezPath {
    crate::kurbo_css::CssBox::new(
        Rect::new(0.0, 0.0, 100.0, 80.0),
        kurbo::Insets::ZERO,
        kurbo::Insets::ZERO,
        0.0,
        crate::kurbo_css::NonUniformRoundedRectRadii::default(),
    )
    .padding_box_path()
}
#[test]
fn actual_css_box_open_and_closed_rectangles_match_in_both_windings() {
    use kurbo::{BezPath, PathEl};
    let produced = sharp_padding_path();
    assert!(matches!(
        produced.elements(),
        [
            PathEl::MoveTo(_),
            PathEl::LineTo(_),
            PathEl::LineTo(_),
            PathEl::LineTo(_)
        ]
    ));
    let corners = [(0.0, 0.0), (100.0, 0.0), (100.0, 80.0), (0.0, 80.0)];
    for reverse in [false, true] {
        for closed in [false, true] {
            let mut path = BezPath::new();
            path.move_to(corners[0]);
            for index in if reverse { [3, 2, 1] } else { [1, 2, 3] } {
                path.line_to(corners[index]);
            }
            if closed {
                path.close_path();
            }
            assert_eq!(exact_rect(&path), Some(Rect::new(0.0, 0.0, 100.0, 80.0)));
        }
    }
    assert_eq!(
        exact_rect(&produced),
        Some(Rect::new(0.0, 0.0, 100.0, 80.0))
    );
}
#[test]
fn open_rectangle_recognition_keeps_nonrectangular_paths_rejected() {
    use kurbo::{BezPath, PathEl, Point};
    let p = |x, y| Point::new(x, y);
    let valid = sharp_padding_path();
    let mut paths = vec![
        vec![
            PathEl::MoveTo(p(0.0, 0.0)),
            PathEl::LineTo(p(100.0, 80.0)),
            PathEl::LineTo(p(100.0, 0.0)),
            PathEl::LineTo(p(0.0, 80.0)),
        ],
        vec![
            PathEl::MoveTo(p(0.0, 0.0)),
            PathEl::LineTo(p(100.0, 0.0)),
            PathEl::LineTo(p(80.0, 80.0)),
            PathEl::LineTo(p(0.0, 80.0)),
        ],
        vec![
            PathEl::MoveTo(p(0.0, 0.0)),
            PathEl::LineTo(p(100.0, 0.0)),
            PathEl::LineTo(p(100.0, 0.0)),
            PathEl::LineTo(p(0.0, 0.0)),
        ],
    ];
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut path = valid.elements().to_vec();
        path[1] = PathEl::LineTo(p(invalid, 0.0));
        paths.push(path);
    }
    for extra in [
        PathEl::ClosePath,
        PathEl::LineTo(p(0.0, 0.0)),
        PathEl::MoveTo(p(2.0, 2.0)),
    ] {
        let mut path = valid.elements().to_vec();
        path.push(PathEl::ClosePath);
        path.push(extra);
        paths.push(path);
    }
    let mut curve = valid.elements().to_vec();
    curve[1] = PathEl::QuadTo(p(50.0, 0.0), p(100.0, 0.0));
    paths.push(curve);
    let mut malformed = valid.elements().to_vec();
    malformed[0] = PathEl::LineTo(p(0.0, 0.0));
    // Kurbo supports PathEl slices starting with LineTo; BezPath::from_vec rejects them.
    assert!(exact_rect(&malformed.as_slice()).is_none());
    for path in paths {
        assert!(exact_rect(&BezPath::from_vec(path)).is_none());
    }
    let rounded = crate::kurbo_css::CssBox::new(
        Rect::new(0.0, 0.0, 100.0, 80.0),
        kurbo::Insets::ZERO,
        kurbo::Insets::ZERO,
        0.0,
        crate::kurbo_css::NonUniformRoundedRectRadii {
            top_left: kurbo::Vec2::new(8.0, 8.0),
            ..Default::default()
        },
    )
    .padding_box_path();
    assert!(exact_rect(&rounded).is_none());
}
#[test]
fn actual_padding_clip_preserves_nested_scale_and_later_overlap_guards() {
    let shared = collector();
    shared.borrow_mut().push(
        Affine::translate((4.0, 6.0)) * Affine::scale(0.5),
        &sharp_padding_path(),
        false,
    );
    target(&shared);
    shared.borrow_mut().pop();
    let trace = shared.borrow().finish().unwrap();
    let node = &trace.targets[&1];
    assert!(!node.unsupported);
    assert!(node.painted);
    assert_eq!(
        node.rectangular_clip,
        Some(Rect::new(10.0, 10.0, 40.0, 30.0))
    );
    assert_eq!(node.later_overlap, Overlap::Clear);
    {
        let _other = Collector::enter(&shared, 99);
        shared
            .borrow_mut()
            .draw(Some(Rect::new(15.0, 15.0, 20.0, 20.0)));
    }
    assert_eq!(
        shared.borrow().finish().unwrap().targets[&1].later_overlap,
        Overlap::Overlap
    );
}
fn collector() -> Shared {
    Rc::new(RefCell::new(Collector::new(
        5,
        None,
        &[(1, 7)],
        Rect::new(0.0, 0.0, 200.0, 100.0),
    )))
}
fn target(shared: &Shared) {
    let _scope = Collector::enter(shared, 1);
    shared.borrow_mut().target(
        1,
        Affine::IDENTITY,
        Rect::new(10.0, 10.0, 40.0, 30.0),
        false,
    );
}
#[test]
fn rectangular_stack_intersects_in_paint_coordinates() {
    let shared = collector();
    shared.borrow_mut().push(
        Affine::translate((5.0, 0.0)),
        &Rect::new(0.0, 0.0, 25.0, 100.0),
        false,
    );
    target(&shared);
    shared.borrow_mut().pop();
    let trace = shared.borrow().finish().unwrap();
    assert_eq!(
        trace.targets[&1].rectangular_clip,
        Some(Rect::new(10.0, 10.0, 30.0, 30.0))
    );
    assert!(!trace.targets[&1].unsupported);
}
#[test]
fn round_clip_and_skipped_layers_are_never_rectangular_success() {
    let shared = collector();
    shared.borrow_mut().push(
        Affine::IDENTITY,
        &kurbo::RoundedRect::from_rect(Rect::new(0.0, 0.0, 50.0, 50.0), 5.0),
        false,
    );
    target(&shared);
    shared.borrow_mut().pop();
    let trace = shared.borrow().finish().unwrap();
    assert!(trace.targets[&1].unsupported);
    assert!(trace.targets[&1].rectangular_clip.is_none());
    shared.borrow_mut().layer_skipped();
    assert!(shared.borrow().finish().unwrap().skipped_layer);
}
#[test]
fn only_later_overlapping_draws_or_unknown_extents_block_the_plane() {
    let shared = collector();
    shared
        .borrow_mut()
        .draw(Some(Rect::new(0.0, 0.0, 200.0, 100.0)));
    target(&shared);
    shared
        .borrow_mut()
        .draw(Some(Rect::new(80.0, 0.0, 90.0, 10.0)));
    assert_eq!(
        shared.borrow().finish().unwrap().targets[&1].later_overlap,
        Overlap::Clear
    );
    shared.borrow_mut().draw(None);
    assert_eq!(
        shared.borrow().finish().unwrap().targets[&1].later_overlap,
        Overlap::Unknown
    );
    shared
        .borrow_mut()
        .draw(Some(Rect::new(15.0, 15.0, 20.0, 20.0)));
    assert_eq!(
        shared.borrow().finish().unwrap().targets[&1].later_overlap,
        Overlap::Overlap
    );
}
#[test]
fn ancestor_unsupported_requests_are_retained_for_target_descendants() {
    let shared = collector();
    {
        let _ancestor = Collector::enter(&shared, 90);
        shared.borrow_mut().unsupported();
        target(&shared);
    }
    assert!(shared.borrow().finish().unwrap().targets[&1].unsupported);
}
#[test]
fn unbalanced_or_aborted_traversal_does_not_publish_trace() {
    let shared = collector();
    shared
        .borrow_mut()
        .push(Affine::IDENTITY, &Rect::new(0.0, 0.0, 100.0, 100.0), false);
    assert!(shared.borrow().finish().is_none());
    shared.borrow_mut().pop();
    shared.borrow_mut().abort();
    assert!(shared.borrow().finish().is_none());
}
#[test]
fn rotation_and_degenerate_path_do_not_become_rectangular_clips() {
    let shared = collector();
    shared.borrow_mut().push(
        Affine::rotate(0.2),
        &Rect::new(0.0, 0.0, 100.0, 100.0),
        false,
    );
    target(&shared);
    shared.borrow_mut().pop();
    assert!(shared.borrow().finish().unwrap().targets[&1].unsupported);
    let mut path = kurbo::BezPath::new();
    path.move_to((0.0, 0.0));
    path.line_to((10.0, 0.0));
    path.line_to((10.0, 10.0));
    path.line_to((10.0, 0.0));
    path.close_path();
    assert!(exact_rect(&path).is_none());
}
#[test]
fn zero_clip_is_hidden_and_overlapping_surface_targets_are_ordered() {
    let shared = collector();
    shared
        .borrow_mut()
        .push(Affine::IDENTITY, &Rect::ZERO, false);
    target(&shared);
    shared.borrow_mut().pop();
    assert!(!shared.borrow().finish().unwrap().targets[&1].painted);
    let shared = Rc::new(RefCell::new(Collector::new(
        5,
        None,
        &[(1, 7), (2, 8)],
        Rect::new(0.0, 0.0, 100.0, 100.0),
    )));
    target(&shared);
    {
        let _second = Collector::enter(&shared, 2);
        shared.borrow_mut().target(
            2,
            Affine::IDENTITY,
            Rect::new(15.0, 15.0, 20.0, 20.0),
            false,
        );
    }
    assert_eq!(
        shared.borrow().finish().unwrap().targets[&1].later_overlap,
        Overlap::Overlap
    );
}

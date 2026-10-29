//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;

#[test]
fn actual_observed_scene_is_unchanged_and_safe_area_offset_is_applied_once() {
    let mut renderer = painted_renderer(
        "<style>body{margin:0}video{display:block;width:120px;height:60px}</style><video id='surface'></video>",
    );
    let node = renderer.surface_node("surface").unwrap().as_u64();
    let original =
        blitz_paint::prepare_scene_checked(&mut renderer.document, 1.0, 320, 180, 11, 17).unwrap();
    let (observed, trace) = blitz_paint::prepare_scene_observed(
        &mut renderer.document,
        1.0,
        (320, 180),
        (11, 17),
        &[(node, 1)],
    )
    .unwrap();
    assert_eq!(original, observed);
    let (_, origin) = blitz_paint::prepare_scene_observed(
        &mut renderer.document,
        1.0,
        (320, 180),
        (0, 0),
        &[(node, 1)],
    )
    .unwrap();
    let rect = trace.unwrap().targets[&node].physical_content_rect.unwrap();
    let zero = origin.unwrap().targets[&node]
        .physical_content_rect
        .unwrap();
    assert_eq!(rect.x0 - zero.x0, 11.0);
    assert_eq!(rect.y0 - zero.y0, 17.0);
    assert_eq!(rect.width(), 120.0);
    assert_eq!(rect.height(), 60.0);
    let placement = crate::infrastructure::engine::surface_slots::RendererClientPlacement {
        viewport: (320, 180),
        insets: [11, 17, 5, 9],
        target: (336, 206),
    };
    placement.validate().unwrap();
    // This trace is already in renderer-target/client pixels, not viewport-local pixels.
    let client = PhysicalClientRect {
        x: rect.x0,
        y: rect.y0,
        width: rect.width(),
        height: rect.height(),
    };
    assert_eq!(client.x - zero.x0, f64::from(placement.insets[0]));
    assert_eq!(client.y - zero.y0, f64::from(placement.insets[1]));
}

#[test]
fn checked_client_revoke_hmr_and_document_busy_never_enter_consumer() {
    let (registrar, video, _) = fixture();
    let stamp = registrar.document.stamp().unwrap();
    let reservation = registrar.reserve_video(&stamp, &video).unwrap();
    let recorded = Rc::new(RefCell::new(None));
    let slot = NativeSurfaceSlot {
        access: Rc::new(RejectAccess(recorded.clone())),
        child: 7,
        binding: Rc::downgrade(&registrar.parent.binding),
        reservation,
        revoked: Cell::new(false),
    };
    let called = Cell::new(false);
    registrar
        .document
        .try_reproject(&stamp, |_| {
            assert!(matches!(
                slot.with_parent_client(|_| called.set(true)),
                Err(super::super::super::NativeParentClientError::Access(
                    NativeSurfaceAccessError::Document(HostAccessError::Busy)
                ))
            ));
            Ok::<_, ()>(())
        })
        .unwrap();
    let stamp = registrar.document.stamp().unwrap();
    registrar
        .document
        .try_reproject(&stamp, |document| {
            document.renderer = crate::infrastructure::engine::NativeRenderer::from_html(
                "<video id='replacement'></video>",
            );
            Ok::<_, ()>(())
        })
        .unwrap();
    assert!(matches!(
        slot.with_parent_client(|_| called.set(true)),
        Err(super::super::super::NativeParentClientError::Access(
            NativeSurfaceAccessError::Slot(SurfaceError::Stale)
        ))
    ));
    slot.revoke();
    assert!(matches!(
        slot.with_parent_client(|_| called.set(true)),
        Err(super::super::super::NativeParentClientError::Access(
            NativeSurfaceAccessError::Slot(SurfaceError::Revoked)
        ))
    ));
    assert!(!called.get());
    assert!(recorded.borrow().is_none());
}

#[test]
fn actual_rounded_and_unknown_url_clips_never_become_rectangular_success() {
    for css in [
        "border-radius:10px;overflow:hidden",
        "clip-path:url(#unsupported)",
    ] {
        let mut renderer = painted_renderer(&format!(
            "<style>body{{margin:0}}div{{width:100px;height:40px;{css}}}video{{display:block;width:120px;height:60px}}</style><div><video id='surface'></video></div>"
        ));
        let node = renderer.surface_node("surface").unwrap().as_u64();
        let (_, trace) = blitz_paint::prepare_scene_observed(
            &mut renderer.document,
            1.0,
            (320, 180),
            (0, 0),
            &[(node, 1)],
        )
        .unwrap();
        assert!(trace.unwrap().targets[&node].unsupported);
    }
}

#[test]
fn actual_hoisted_overlay_blocks_the_child_plane_and_focus_stays_logical() {
    let mut renderer = painted_renderer(
        "<style>body{margin:0}video{display:block;width:120px;height:60px}button{position:absolute;top:0;left:0;z-index:3;width:50px;height:20px;background:red}</style><video id='surface'></video><button id='control'>control</button>",
    );
    let node = renderer.surface_node("surface").unwrap().as_u64();
    let button = renderer.document.get_element_by_id("control").unwrap();
    renderer.document.set_focus_to(button);
    let (_, trace) = blitz_paint::prepare_scene_observed(
        &mut renderer.document,
        1.0,
        (320, 180),
        (0, 0),
        &[(node, 1)],
    )
    .unwrap();
    let trace = trace.unwrap();
    assert_ne!(
        trace.targets[&node].later_overlap,
        blitz_paint::Overlap::Clear
    );
    assert_eq!(trace.focused_node, Some(button.as_u64()));
}

#[test]
fn committed_trace_becomes_stale_only_when_its_strong_owner_is_dropped() {
    let (registrar, _, _) = fixture();
    let stamp = registrar.document.stamp().unwrap();
    let window = registrar.parent.stamp().unwrap();
    let mut renderer = painted_renderer(
        "<style>video{width:120px;height:60px}</style><video id='surface'></video>",
    );
    let node = renderer.surface_node("surface").unwrap().as_u64();
    let owner = Rc::new(());
    let weak_owner = Rc::downgrade(&owner);
    renderer
        .track_surface(
            Rc::new(PaintContext {
                window,
                revision: Rc::new(Cell::new(1)),
            }),
            node,
            9,
            weak_owner.clone(),
        )
        .unwrap();
    let scope = renderer.surface_render_scope(&stamp).unwrap();
    assert_eq!(scope.targets, vec![(node, 9)]);
    let (_, trace) = blitz_paint::prepare_scene_observed(
        &mut renderer.document,
        1.0,
        (320, 180),
        (0, 0),
        &scope.targets,
    )
    .unwrap();
    let frame = crate::domain::graphics::FrameTicket {
        context_id: "headless-owner-drop-validation".into(),
        context_generation: 1,
        scene_generation: 1,
    };
    // Retain the getter's result across drop to detect accidental strong ownership in snapshots.
    renderer
        .commit_surface_paint(scope.clone(), frame.clone(), trace.unwrap(), None)
        .unwrap();
    let committed = renderer.committed_surface_paint(&stamp).unwrap();
    assert_eq!(committed.scope, scope);
    assert_eq!(committed.frame, frame);
    assert_eq!(Rc::strong_count(&owner), 1);

    drop(owner);

    assert_eq!(weak_owner.strong_count(), 0);
    assert!(weak_owner.upgrade().is_none());
    assert!(matches!(
        renderer.committed_surface_paint(&stamp),
        Err(SurfaceError::Stale)
    ));
    // The original successful observation was not cleared by an earlier failed commit.
    assert_eq!(committed.scope.targets, vec![(node, 9)]);
    let mut current_scope = renderer.surface_render_scope(&stamp).unwrap();
    assert!(current_scope.targets.is_empty());
    current_scope.targets = scope.targets.clone();
    assert_eq!(current_scope, scope);
}

#[test]
fn committed_trace_rejects_changed_geometry_source_and_expired_slot_generation() {
    let (registrar, _, _) = fixture();
    let stamp = registrar.document.stamp().unwrap();
    let window = registrar.parent.stamp().unwrap();
    let mut renderer = painted_renderer(
        "<style>video{width:120px;height:60px}</style><video id='surface'></video>",
    );
    let node = renderer.surface_node("surface").unwrap().as_u64();
    let owner = Rc::new(());
    let revision = Rc::new(Cell::new(1));
    renderer
        .track_surface(
            Rc::new(PaintContext {
                window,
                revision: revision.clone(),
            }),
            node,
            9,
            Rc::downgrade(&owner),
        )
        .unwrap();
    let scope = renderer.surface_render_scope(&stamp).unwrap();
    let (_, trace) = blitz_paint::prepare_scene_observed(
        &mut renderer.document,
        1.0,
        (320, 180),
        (0, 0),
        &scope.targets,
    )
    .unwrap();
    let frame = crate::domain::graphics::FrameTicket {
        context_id: "headless-submission-validation".into(),
        context_generation: 1,
        scene_generation: 1,
    };
    // Private commit-port unit: real checked submit is tested separately; this is not an OS ACK.
    renderer
        .commit_surface_paint(scope.clone(), frame.clone(), trace.clone().unwrap(), None)
        .unwrap();
    assert_eq!(
        renderer.committed_surface_paint(&stamp).unwrap().frame,
        frame
    );
    let mut stale = stamp.clone();
    stale.document_revision += 1;
    assert!(renderer.committed_surface_paint(&stale).is_err());
    revision.set(2);
    assert!(renderer.committed_surface_paint(&stamp).is_err());
    assert!(
        renderer
            .commit_surface_paint(scope, frame, trace.unwrap(), None)
            .is_err()
    );
    revision.set(1);
    assert!(renderer.committed_surface_paint(&stamp).is_err());
    drop(owner);
    assert!(renderer.committed_surface_paint(&stamp).is_err());
}

#[test]
fn rejected_original_cleanup_access_does_not_revoke_or_enter_consumer() {
    let (registrar, video, _) = fixture();
    let stamp = registrar.document.stamp().unwrap();
    let reservation = registrar.reserve_video(&stamp, &video).unwrap();
    let window = reservation.window;
    let recorded = Rc::new(RefCell::new(None));
    let slot = NativeSurfaceSlot {
        access: Rc::new(RejectAccess(recorded.clone())),
        child: 7,
        binding: Rc::downgrade(&registrar.parent.binding),
        reservation,
        revoked: Cell::new(false),
    };
    let called = Cell::new(false);
    for (child, attempt) in [(8, 3), (7, 2)] {
        let request = CleanupRequest {
            stamp: CleanupStamp {
                window,
                child,
                revocation: 2,
                attempt,
            },
            reason: RevocationReason::ExplicitRelease,
        };
        let result = slot.with_cleanup_surface(request, |_| {
            called.set(true);
        });
        assert_eq!(result, Err(LifecycleError::Stale));
        assert!(!slot.revoked.get());
        assert!(!called.get());
        assert_eq!(recorded.borrow().unwrap().stamp, request.stamp);
    }
    // Rejection must not undo an admission revocation the owner already requested explicitly.
    slot.revoke();
    let request = recorded.borrow().unwrap();
    assert_eq!(
        slot.with_cleanup_surface(request, |_| ()),
        Err(LifecycleError::Stale)
    );
    assert!(slot.revoked.get());
}

#[test]
fn readonly_window_and_child_provenance_do_not_grant_live_parent_access() {
    let (registrar, video, _) = fixture();
    let (foreign, _, _) = fixture();
    let stamp = registrar.document.stamp().unwrap();
    let reservation = registrar.reserve_video(&stamp, &video).unwrap();
    let expected = reservation.window;
    let slot = NativeSurfaceSlot {
        access: Rc::new(RejectAccess(Rc::new(RefCell::new(None)))),
        child: 7,
        binding: Rc::downgrade(&registrar.parent.binding),
        reservation,
        revoked: Cell::new(false),
    };
    assert_eq!(slot.window(), expected);
    assert_eq!(slot.child(), 7);
    assert_ne!(slot.window(), foreign.parent.stamp().unwrap());
    // Metadata is readable, but this fixture has no OS parent or committed surface frame.
    assert!(
        slot.with_surface(|_, _| panic!("metadata cannot mint access"))
            .is_err()
    );
}

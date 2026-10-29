//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;

#[test]
fn actual_clipped_text_stroke_and_shadow_keep_scene_pixels_and_video_separation() {
    use anyrender::{ImageRenderer, PaintScene, recording::RenderCommand};
    let mut renderer = actual_unknown_ink_renderer("overflow:hidden");
    let node = renderer.surface_node("surface").unwrap().as_u64();
    let plain =
        blitz_paint::prepare_scene_checked(&mut renderer.document, 1.0, 320, 180, 0, 0).unwrap();
    let (observed, trace) = blitz_paint::prepare_scene_observed(
        &mut renderer.document,
        1.0,
        (320, 180),
        (0, 0),
        &[(node, 1)],
    )
    .unwrap();
    assert_eq!(plain, observed);
    assert!(
        observed.commands.iter().any(
            |command| matches!(command, RenderCommand::GlyphRun(run) if !run.glyphs.is_empty())
        )
    );
    assert!(
        observed
            .commands
            .iter()
            .any(|command| matches!(command, RenderCommand::Stroke(_)))
    );
    assert!(
        observed
            .commands
            .iter()
            .any(|command| matches!(command, RenderCommand::BoxShadow(_)))
    );
    let trace = trace.unwrap();
    let target = &trace.targets[&node];
    assert!(target.painted);
    assert!(!target.unsupported);
    assert_eq!(target.physical_content_rect, target.rectangular_clip);
    assert_eq!(target.later_overlap, blitz_paint::Overlap::Clear);
    let mut expected = Vec::new();
    let mut actual = Vec::new();
    let mut cpu = anyrender_vello_cpu::VelloCpuImageRenderer::new(320, 180);
    cpu.render_to_vec(
        |scene| scene.append_scene(plain, Default::default()),
        &mut expected,
    );
    let mut cpu = anyrender_vello_cpu::VelloCpuImageRenderer::new(320, 180);
    cpu.render_to_vec(
        |scene| scene.append_scene(observed, Default::default()),
        &mut actual,
    );
    assert_eq!(actual, expected);
    assert!(actual.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0));
}

#[test]
fn actual_unknown_ink_without_disjoint_proven_clip_remains_unsafe() {
    for region in [
        "",
        "overflow:hidden;position:absolute;left:0;top:0;margin-top:0",
        "overflow:hidden;border-radius:8px",
        "overflow:hidden;opacity:0.5",
    ] {
        let mut renderer = actual_unknown_ink_renderer(region);
        let node = renderer.surface_node("surface").unwrap().as_u64();
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
            blitz_paint::Overlap::Clear,
            "region={region}"
        );
    }
}

#[test]
fn actual_sharp_overflow_clip_keeps_video_bounds_and_observed_scene() {
    let mut renderer = painted_renderer(
        "<style>body{margin:0}.outer{width:240px;height:120px;overflow:hidden}.inner{width:240px;height:120px;overflow:hidden}video{display:block;width:120px;height:60px}.guide{height:40px;background:red}</style><div class='outer'><div class='inner'><video id='surface'></video><div class='guide'></div></div></div>",
    );
    let node = renderer.surface_node("surface").unwrap().as_u64();
    let plain =
        blitz_paint::prepare_scene_checked(&mut renderer.document, 1.0, 320, 180, 11, 17).unwrap();
    let (observed, trace) = blitz_paint::prepare_scene_observed(
        &mut renderer.document,
        1.0,
        (320, 180),
        (11, 17),
        &[(node, 1)],
    )
    .unwrap();
    assert_eq!(plain, observed);
    let trace = trace.unwrap();
    let target = &trace.targets[&node];
    assert!(target.painted);
    assert!(!target.unsupported);
    let rect = target.physical_content_rect.unwrap();
    assert_eq!(
        (rect.x0, rect.y0, rect.width(), rect.height()),
        (11.0, 17.0, 120.0, 60.0)
    );
    assert_eq!(target.rectangular_clip, Some(rect));
    assert_eq!(target.later_overlap, blitz_paint::Overlap::Clear);
}

#[test]
fn actual_rounded_clip_and_later_video_overlay_remain_rejected() {
    for rounded in [false, true] {
        let clip = if rounded { "border-radius:12px" } else { "" };
        let overlay = if rounded {
            ""
        } else {
            "position:absolute;left:0;top:0;width:120px"
        };
        let mut renderer = painted_renderer(&format!(
            "<style>body{{margin:0}}.outer{{position:relative;width:240px;height:120px;overflow:hidden;{clip}}}video{{display:block;width:120px;height:60px}}.guide{{height:40px;background:red;{overlay}}}</style><div class='outer'><video id='surface'></video><div class='guide'></div></div>"
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
        let trace = trace.unwrap();
        let target = &trace.targets[&node];
        if rounded {
            assert!(target.unsupported);
        } else {
            assert!(!target.unsupported);
            assert_eq!(target.later_overlap, blitz_paint::Overlap::Overlap);
        }
    }
}

#[test]
fn only_original_declared_video_and_full_admission_stamp_can_reserve() {
    let (registrar, video, other) = fixture();
    let stamp = registrar.document.stamp().unwrap();
    assert!(matches!(
        registrar.reserve_video(&stamp, &other),
        Err(NativeSurfaceAccessError::Slot(SurfaceError::NotVideo))
    ));
    let mut stale = stamp.clone();
    stale.document_revision += 1;
    assert!(matches!(
        registrar.reserve_video(&stale, &video),
        Err(NativeSurfaceAccessError::Slot(SurfaceError::Stale))
    ));
    assert!(registrar.reserve_video(&stamp, &video).is_ok());
}

#[test]
fn active_duplicate_rejects_and_drop_reissues_a_new_slot_generation() {
    let (registrar, video, _) = fixture();
    let stamp = registrar.document.stamp().unwrap();
    let first = registrar.reserve_video(&stamp, &video).unwrap();
    let generation = first.reservation.generation();
    assert!(matches!(
        registrar.reserve_video(&stamp, &video),
        Err(NativeSurfaceAccessError::Slot(SurfaceError::Duplicate))
    ));
    drop(first);
    assert!(
        registrar
            .reserve_video(&stamp, &video)
            .unwrap()
            .reservation
            .generation()
            > generation
    );
}

#[test]
fn retained_node_survives_ordinary_revision_but_registration_requires_original_full_stamp() {
    let (registrar, video, _) = fixture();
    let stamp = registrar.document.stamp().unwrap();
    let reservation = registrar.reserve_video(&stamp, &video).unwrap();
    registrar
        .document
        .try_reproject(&stamp, |_| Ok::<_, ()>(()))
        .unwrap();
    reservation.validate_node().unwrap();
    let called = Cell::new(false);
    let result = registrar.register_reserved(reservation, |_, _, _| {
        called.set(true);
        Ok(Child)
    });
    assert!(matches!(
        result,
        Err(NativeSurfaceRegistrationFailure::Access(
            NativeSurfaceAccessError::Slot(SurfaceError::Stale)
        ))
    ));
    assert!(!called.get());
}

#[test]
fn replacing_the_original_renderer_never_remaps_old_ids_into_a_new_document() {
    let (registrar, video, _) = fixture();
    let stamp = registrar.document.stamp().unwrap();
    let reservation = registrar.reserve_video(&stamp, &video).unwrap();
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
        reservation.validate_node(),
        Err(NativeSurfaceAccessError::Slot(SurfaceError::Stale))
    ));
}

#[test]
fn replacing_only_public_html_document_rejects_reused_node_ids_in_the_same_renderer() {
    let (registrar, video, _) = fixture();
    let stamp = registrar.document.stamp().unwrap();
    let reservation = registrar.reserve_video(&stamp, &video).unwrap();
    registrar
        .document
        .try_reproject(&stamp, |document| {
            let replacement = InteractiveDocument::new(
                document.source.clone(),
                "video { display:block; width:120px; height:60px; }",
                document.viewport.clone(),
            )
            .unwrap();
            // This is the original counterexample: same semantic id + NodeId + renderer authority.
            assert_eq!(
                replacement.renderer.surface_node(&video).unwrap(),
                reservation.node
            );
            assert_ne!(
                replacement.renderer.document.id(),
                reservation.dom_document_id
            );
            document.renderer.document = replacement.renderer.document;
            assert!(Weak::ptr_eq(
                &reservation.renderer,
                &document.renderer.surface_identity()
            ));
            assert_eq!(
                document.renderer.surface_node(&video).unwrap(),
                reservation.node
            );
            Ok::<_, ()>(())
        })
        .unwrap();
    assert!(matches!(
        reservation.validate_node(),
        Err(NativeSurfaceAccessError::Slot(SurfaceError::Stale))
    ));
}

#[test]
fn foreign_registrar_does_not_invoke_factory_or_mint_parent_access() {
    let (first, video, _) = fixture();
    let (foreign, _, _) = fixture();
    let stamp = first.document.stamp().unwrap();
    let reservation = first.reserve_video(&stamp, &video).unwrap();
    let called = Cell::new(false);
    let result = foreign.register_reserved(reservation, |_, _, _| {
        called.set(true);
        Ok(Child)
    });
    assert!(matches!(
        result,
        Err(NativeSurfaceRegistrationFailure::Access(_))
    ));
    assert!(!called.get());
}

#[test]
fn real_box_observation_does_not_claim_effective_clip_overlay_or_os_ack() {
    let (registrar, video, _) = fixture();
    let stamp = registrar.document.stamp().unwrap();
    let reservation = registrar.reserve_video(&stamp, &video).unwrap();
    registrar
        .document
        .try_with(|document| {
            let observed = document
                .renderer
                .measure_surface(&video, reservation.node)
                .unwrap();
            assert_eq!(observed.rect.width, 120.0);
            assert_eq!(observed.rect.height, 60.0);
            assert_eq!(observed.clip, SurfaceClip::Unknown);
            assert_eq!(observed.plane, SurfacePlane::Unknown);
            assert_eq!(observed.visibility, SurfaceVisibility::Unknown);
        })
        .unwrap();
}

#[test]
fn not_ready_parent_registration_preserves_existing_failure_boundary() {
    let (registrar, video, _) = fixture();
    let stamp = registrar.document.stamp().unwrap();
    let reservation = registrar.reserve_video(&stamp, &video).unwrap();
    let called = Cell::new(false);
    let result = registrar.register_reserved(reservation, |_, _, _| {
        called.set(true);
        Ok(Child)
    });
    // There is deliberately no OS window in this headless fixture. No surface factory is entered.
    assert!(matches!(
        result,
        Err(NativeSurfaceRegistrationFailure::Child(_))
    ));
    assert!(!called.get());
}

#[test]
fn deleting_and_recreating_the_same_video_id_never_reuses_the_old_node_authority() {
    let (registrar, video, _) = fixture();
    let stamp = registrar.document.stamp().unwrap();
    let reservation = registrar.reserve_video(&stamp, &video).unwrap();
    registrar
        .document
        .try_reproject(&stamp, |document| {
            use blitz_dom::{LocalName, Namespace, QualName};
            let root = document.renderer.document.root_element().id;
            let mut dom = document.renderer.document.mutate();
            dom.remove_and_drop_node(reservation.node);
            let new = dom.create_element(
                QualName::new(
                    None,
                    Namespace::from("http://www.w3.org/1999/xhtml"),
                    LocalName::from("video"),
                ),
                vec![],
            );
            dom.set_attribute(
                new,
                QualName::new(None, Namespace::from(""), LocalName::from("id")),
                &video,
            );
            dom.append_children(root, &[new]);
            assert_ne!(new, reservation.node);
            Ok::<_, ()>(())
        })
        .unwrap();
    assert!(reservation.validate_node().is_err());
}

#[test]
fn revoked_slot_blocks_new_use_but_forwards_original_full_cleanup_request() {
    let (registrar, video, _) = fixture();
    let stamp = registrar.document.stamp().unwrap();
    let reservation = registrar.reserve_video(&stamp, &video).unwrap();
    let recorded = Rc::new(RefCell::new(None));
    let window = reservation.window;
    let slot = NativeSurfaceSlot {
        access: Rc::new(RejectAccess(recorded.clone())),
        child: 7,
        binding: Rc::downgrade(&registrar.parent.binding),
        reservation,
        revoked: Cell::new(false),
    };
    slot.revoke();
    assert!(slot.with_surface(|_, _| panic!("revoked")).is_err());
    let request = CleanupRequest {
        stamp: CleanupStamp {
            window,
            child: 7,
            revocation: 2,
            attempt: 3,
        },
        reason: RevocationReason::ExplicitRelease,
    };
    assert_eq!(
        slot.with_cleanup_surface(request, |_| -> () { panic!("rejected original access") }),
        Err(LifecycleError::Stale)
    );
    let actual = recorded.borrow().unwrap();
    assert_eq!(actual.stamp, request.stamp);
    assert_eq!(actual.reason, request.reason);
}

use super::*;
fn actual_unknown_ink_renderer(region: &str) -> crate::infrastructure::engine::NativeRenderer {
    painted_renderer(&format!(
        "<style>body{{margin:0;font:16px sans-serif}}video{{display:block;width:120px;height:60px}}.region{{margin-top:12px;width:240px;height:80px;{region}}}p{{margin:0}}button:disabled{{display:block;height:20px;font-size:12px;opacity:0.5}}.shadow{{width:60px;height:10px;box-shadow:0 0 5px red}}</style><video id='surface'></video><div class='region'><p>Real guide text</p><button disabled>Disabled text</button><div class='shadow'></div><svg width='24' height='24'><path d='M2 2L20 20' stroke='black' stroke-width='2' fill='none'/></svg></div>"
    ))
}
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
use crate::{
    domain::Viewport,
    infrastructure::window::{forwarding::ForwardingSeed, parent::ParentBinding},
};
use std::{io, task::Waker, time::Duration};
#[derive(Clone, Copy)]
struct Clock;
impl MonotonicClock for Clock {
    fn now(&self) -> Duration {
        Duration::ZERO
    }
}
struct Child;
impl NativeChild for Child {
    type Error = io::Error;
    fn parent_lost(&mut self) {}
    fn begin_cleanup(
        &mut self,
        _: CleanupRequest,
        _: CompletionSignal,
    ) -> Result<CleanupProgress, io::Error> {
        Ok(CleanupProgress::Complete)
    }
    fn observe_cleanup(&mut self, _: CleanupRequest) -> Result<CleanupProgress, io::Error> {
        Ok(CleanupProgress::Complete)
    }
}
fn fixture() -> (NativeSurfaceRegistrar<Child>, String, String) {
    let source = meari::parse(
        "type Props = {}; export function Surface(props: Props) { return <div><video /><div /></div>; }",
        meari::application::Language::Tsx,
    )
    .unwrap();
    let document = InteractiveDocument::new(
        source,
        "video { display:block; width:120px; height:60px; }",
        Viewport {
            width: 320,
            height: 180,
            dark: false,
        },
    )
    .unwrap();
    let video = document
        .elements()
        .iter()
        .find(|element| element.dom_tag == "video")
        .unwrap()
        .id
        .clone();
    let other = document
        .elements()
        .iter()
        .find(|element| element.dom_tag == "div")
        .unwrap()
        .id
        .clone();
    let document = NativeDocumentHandle::new(document, 1).unwrap();
    let seed = ForwardingSeed::new(document.id());
    let coordinator = Rc::new(
        WindowLifecycle::new(
            LifecycleLimits::new(4, Duration::from_secs(1)).unwrap(),
            Clock,
            Waker::noop().clone(),
        )
        .unwrap(),
    );
    coordinator.ready(ReadyKind::NewWindow).unwrap();
    let parent = NativeChildRegistrar {
        binding: ParentBinding::new(document.id(), seed, None),
        coordinator,
    };
    (NativeSurfaceRegistrar::new(document, parent), video, other)
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
struct RejectAccess(Rc<RefCell<Option<CleanupRequest>>>);
impl ParentAccess for RejectAccess {
    fn visit(
        &self,
        request: Option<CleanupRequest>,
        _: &mut dyn FnMut(&NativeScopedParent<'_>),
    ) -> Result<(), LifecycleError> {
        *self.0.borrow_mut() = request;
        Err(LifecycleError::Stale)
    }
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
fn painted_renderer(html: &str) -> crate::infrastructure::engine::NativeRenderer {
    use crate::application::Renderer;
    let mut renderer = crate::infrastructure::engine::NativeRenderer::from_html(html);
    renderer
        .layout(
            &Viewport {
                width: 320,
                height: 180,
                dark: false,
            },
            0.0,
        )
        .unwrap();
    renderer
}
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
                Err(super::super::NativeParentClientError::Access(
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
        Err(super::super::NativeParentClientError::Access(
            NativeSurfaceAccessError::Slot(SurfaceError::Stale)
        ))
    ));
    slot.revoke();
    assert!(matches!(
        slot.with_parent_client(|_| called.set(true)),
        Err(super::super::NativeParentClientError::Access(
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
struct PaintContext {
    window: WindowStamp,
    revision: Rc<Cell<u64>>,
}
impl crate::infrastructure::engine::surface_slots::SurfaceContext for PaintContext {
    fn current(&self) -> Result<(WindowStamp, u64), SurfaceError> {
        Ok((self.window, self.revision.get()))
    }
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

use super::*;
use crate::{
    application::window_lifecycle::*,
    domain::{Viewport, graphics::FrameTicket, surface_slot::*},
    infrastructure::{document::InteractiveDocument, native_host::NativeDocumentHandle},
};
use std::{cell::Cell, io, task::Waker, time::Duration};

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
#[derive(Clone, Copy)]
struct Clock;
impl MonotonicClock for Clock {
    fn now(&self) -> Duration {
        Duration::ZERO
    }
}
fn state() -> ClientState {
    let source = meari::parse(
        "type Props={};export function App(props:Props){return <video />;}",
        meari::application::Language::Tsx,
    )
    .unwrap();
    let document = NativeDocumentHandle::new(
        InteractiveDocument::new(
            source,
            "",
            Viewport {
                width: 320,
                height: 180,
                dark: false,
            },
        )
        .unwrap(),
        1,
    )
    .unwrap();
    let lifecycle = WindowLifecycle::<Child>::new(
        LifecycleLimits::new(1, Duration::from_secs(1)).unwrap(),
        Clock,
        Waker::noop().clone(),
    )
    .unwrap();
    lifecycle.ready(ReadyKind::NewWindow).unwrap();
    ClientState {
        snapshot: NativeSurfaceSnapshot {
            frame: FrameTicket {
                context_id: "checked-parent-test".into(),
                context_generation: 1,
                scene_generation: 1,
            },
            logical_focus: SurfaceLogicalFocus {
                focused_node: None,
                surface_focusable: Some(false),
            },
            document: document.stamp().unwrap(),
            window: lifecycle.stamp().unwrap(),
            slot_generation: 1,
            layout_revision: 1,
            host_geometry_revision: 1,
            physical_client_rect: Some(PhysicalClientRect {
                x: 7.0,
                y: 11.0,
                width: 120.0,
                height: 60.0,
            }),
            device_scale: 1.0,
            layout_zoom: 1.0,
            visibility: SurfaceVisibility::Visible,
            clip: SurfaceClip::Rectangular(PhysicalClientRect {
                x: 7.0,
                y: 11.0,
                width: 120.0,
                height: 60.0,
            }),
            plane: SurfacePlane::RectangularChildWindow,
            focus: SurfaceFocus::Unknown,
        },
        placement: RendererClientPlacement {
            viewport: (320, 180),
            insets: [7, 11, 13, 17],
            target: (340, 208),
        },
    }
}
fn copy_state(value: &ClientState) -> ClientState {
    ClientState {
        snapshot: value.snapshot.clone(),
        placement: value.placement,
    }
}
fn observed() -> ClientReadback {
    ClientReadback {
        size: (340, 208),
        screen_origin: (-1280, -40),
    }
}

#[test]
fn same_frame_accepts_negative_origin_without_adding_paint_offset_again() {
    let expected = state();
    let before = expected.snapshot.physical_client_rect;
    let reads = Cell::new(0);
    assert_eq!(
        validate_observation(&expected, None, &|| Ok(copy_state(&expected)), &|| {
            reads.set(reads.get() + 1);
            Ok(observed())
        })
        .unwrap(),
        observed()
    );
    assert_eq!(reads.get(), 2);
    assert_eq!(expected.snapshot.physical_client_rect, before);
    assert_eq!(expected.snapshot.focus, SurfaceFocus::Unknown);
    assert!(
        expected
            .snapshot
            .require_rectangular_child_window()
            .is_err()
    );
}
#[test]
fn stale_frame_geometry_host_slot_and_renderer_placement_stop_before_os_read() {
    let expected = state();
    for change in 0..8 {
        let reads = Cell::new(0);
        let current = || {
            let mut next = copy_state(&expected);
            match change {
                0 => next.snapshot.frame.scene_generation += 1,
                1 => next.snapshot.frame.context_generation += 1,
                2 => next.snapshot.host_geometry_revision += 1,
                3 => next.snapshot.document.document_revision += 1,
                4 => next.snapshot.slot_generation += 1,
                5 => next.placement.insets[0] += 1,
                6 => next.snapshot.device_scale = 2.0,
                _ => next.snapshot.logical_focus.focused_node = Some(99),
            };
            Ok(next)
        };
        assert!(matches!(
            validate_observation(&expected, None, &current, &|| {
                reads.set(reads.get() + 1);
                Ok(observed())
            }),
            Err(NativeParentClientError::Access(
                NativeSurfaceAccessError::Slot(SurfaceError::Stale)
            ))
        ));
        assert_eq!(reads.get(), 0);
    }
}
#[test]
fn original_access_errors_are_preserved_without_readback_or_fallback() {
    let expected = state();
    for error in [
        LifecycleError::Closing,
        LifecycleError::Stale,
        LifecycleError::Busy,
        LifecycleError::Closed,
    ] {
        let reads = Cell::new(0);
        let result = validate_observation(
            &expected,
            None,
            &|| Err(NativeSurfaceAccessError::Parent(error).into()),
            &|| {
                reads.set(reads.get() + 1);
                Ok(observed())
            },
        );
        assert!(
            matches!(result,Err(NativeParentClientError::Access(NativeSurfaceAccessError::Parent(actual))) if actual==error)
        );
        assert_eq!(reads.get(), 0);
    }
}
#[test]
fn readback_failure_and_resize_do_not_reuse_an_old_success() {
    let expected = state();
    assert!(
        matches!(validate_observation(&expected,Some(observed()),&||Ok(copy_state(&expected)),&||Err(NativeParentClientError::OsReadback("original failure".into()))),Err(NativeParentClientError::OsReadback(message)) if message=="original failure")
    );
    for changed in [
        ClientReadback {
            size: (0, 208),
            ..observed()
        },
        ClientReadback {
            size: (341, 208),
            ..observed()
        },
        ClientReadback {
            screen_origin: (-1279, -40),
            ..observed()
        },
    ] {
        assert!(matches!(
            validate_observation(
                &expected,
                Some(observed()),
                &|| Ok(copy_state(&expected)),
                &|| Ok(changed)
            ),
            Err(NativeParentClientError::PlacementMismatch)
        ));
    }
}
#[test]
fn scope_or_os_changes_during_readback_are_rejected() {
    let expected = state();
    let changed = Cell::new(false);
    let result = validate_observation(
        &expected,
        None,
        &|| {
            let mut next = copy_state(&expected);
            if changed.get() {
                next.snapshot.host_geometry_revision += 1;
            }
            Ok(next)
        },
        &|| {
            changed.set(true);
            Ok(observed())
        },
    );
    assert!(matches!(
        result,
        Err(NativeParentClientError::Access(
            NativeSurfaceAccessError::Slot(SurfaceError::Stale)
        ))
    ));
    let reads = Cell::new(0);
    let result = validate_observation(&expected, None, &|| Ok(copy_state(&expected)), &|| {
        reads.set(reads.get() + 1);
        let mut value = observed();
        value.screen_origin.0 += reads.get();
        Ok(value)
    });
    assert!(matches!(
        result,
        Err(NativeParentClientError::PlacementMismatch)
    ));
}

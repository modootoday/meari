//! UI-thread slot authority. Desired layout is not an OS surface acknowledgement.
use super::parent::{NativeChildRegistrar, NativeParentLease, NativeScopedParent, ParentAccess};
use crate::{
    application::{
        surface_slot::{Reservation, SurfaceRegistry},
        window_lifecycle::*,
    },
    domain::surface_slot::*,
    infrastructure::{
        document::InteractiveDocument,
        native_host::{HostAccessError, HostLifecycle, HostStamp, NativeDocumentHandle},
    },
};
use blitz_dom::NodeId;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

#[derive(Debug)]
pub enum NativeSurfaceAccessError {
    Slot(SurfaceError),
    Document(HostAccessError),
    Parent(LifecycleError),
}
impl std::fmt::Display for NativeSurfaceAccessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native surface access: {self:?}")
    }
}
impl std::error::Error for NativeSurfaceAccessError {}
impl From<SurfaceError> for NativeSurfaceAccessError {
    fn from(error: SurfaceError) -> Self {
        Self::Slot(error)
    }
}
impl From<HostAccessError> for NativeSurfaceAccessError {
    fn from(error: HostAccessError) -> Self {
        Self::Document(error)
    }
}
impl From<LifecycleError> for NativeSurfaceAccessError {
    fn from(error: LifecycleError) -> Self {
        Self::Parent(error)
    }
}
#[derive(Debug)]
pub enum NativeSurfaceRegistrationFailure<E> {
    Access(NativeSurfaceAccessError),
    Child(RegistrationFailure<E>),
}

/// Metadata only. Public fields cannot mint an attachment, parent lease or Destroy receipt.
#[derive(Clone, Debug)]
pub struct NativeSurfaceSnapshot {
    pub frame: crate::domain::graphics::FrameTicket,
    pub logical_focus: SurfaceLogicalFocus,
    pub document: HostStamp,
    pub window: WindowStamp,
    pub slot_generation: u64,
    /// Monotonic observation revision, independent of host/document counters.
    pub layout_revision: u64,
    pub host_geometry_revision: u64,
    pub physical_client_rect: Option<PhysicalClientRect>,
    pub device_scale: f64,
    pub layout_zoom: f64,
    pub visibility: SurfaceVisibility,
    pub clip: SurfaceClip,
    pub plane: SurfacePlane,
    pub focus: SurfaceFocus,
}
impl NativeSurfaceSnapshot {
    pub fn require_rectangular_child_window(&self) -> Result<(), SurfaceError> {
        if self.visibility != SurfaceVisibility::Visible {
            return Err(SurfaceError::Unsupported(
                "effective paint visibility is not verified",
            ));
        }
        self.physical_client_rect
            .ok_or(SurfaceError::NotReady)?
            .validate()?;
        if !matches!(self.clip, SurfaceClip::Rectangular(_)) {
            return Err(SurfaceError::Unsupported(
                "effective clip is not verified rectangular",
            ));
        }
        if self.plane != SurfacePlane::RectangularChildWindow {
            return Err(SurfaceError::Unsupported(
                "child-window stacking/overlay plane is not verified",
            ));
        }
        if self.focus != SurfaceFocus::RenderOnly {
            return Err(SurfaceError::Unsupported(
                "render-only focus contract is not verified",
            ));
        }
        Ok(())
    }
}
/// No Clone/Send/serde/raw constructor. Dropping unconsumed authority releases its reservation.
/// ```compile_fail
/// use meari_native::infrastructure::window::NativeSurfaceReservation;
/// fn duplicate(value: NativeSurfaceReservation) { let _ = value.clone(); }
/// ```
/// ```compile_fail
/// use meari_native::infrastructure::window::NativeSurfaceReservation;
/// fn require_send<T: Send>() {}
/// require_send::<NativeSurfaceReservation>();
/// ```
pub struct NativeSurfaceReservation {
    document: NativeDocumentHandle,
    expected: HostStamp,
    window: WindowStamp,
    semantic_id: String,
    node: NodeId,
    renderer: Weak<()>,
    dom_document_id: usize,
    registry: Weak<RefCell<SurfaceRegistry>>,
    reservation: Reservation,
    _paint_owner: Rc<()>,
}
struct BoundSurfaceContext<C: NativeChild> {
    binding: Weak<super::parent::ParentBinding>,
    coordinator: Weak<WindowLifecycle<C>>,
}
impl<C: NativeChild> crate::infrastructure::engine::surface_slots::SurfaceContext
    for BoundSurfaceContext<C>
{
    fn current(&self) -> Result<(WindowStamp, u64), SurfaceError> {
        let coordinator = self.coordinator.upgrade().ok_or(SurfaceError::Revoked)?;
        if coordinator.phase().map_err(|_| SurfaceError::Busy)? != WindowPhase::Ready {
            return Err(SurfaceError::NotReady);
        }
        let binding = self.binding.upgrade().ok_or(SurfaceError::Revoked)?;
        Ok((
            coordinator.stamp().map_err(|_| SurfaceError::Busy)?,
            binding
                .geometry()
                .map_err(|_| SurfaceError::Busy)?
                .ok_or(SurfaceError::NotReady)?
                .revision,
        ))
    }
}
pub struct NativeSurfaceRegistrar<C: NativeChild> {
    document: NativeDocumentHandle,
    parent: NativeChildRegistrar<C>,
}
impl<C: NativeChild> NativeSurfaceRegistrar<C> {
    pub(super) fn new(document: NativeDocumentHandle, parent: NativeChildRegistrar<C>) -> Self {
        Self { document, parent }
    }
    pub fn reserve_video(
        &self,
        expected: &HostStamp,
        id: &str,
    ) -> Result<NativeSurfaceReservation, NativeSurfaceAccessError> {
        if self.document.id() != self.parent.binding.host
            || self.document.lifecycle() != HostLifecycle::Open
        {
            return Err(SurfaceError::Revoked.into());
        }
        let current = self.document.stamp()?;
        if &current != expected {
            return Err(SurfaceError::Stale.into());
        }
        if self.parent.phase()? != WindowPhase::Ready {
            return Err(SurfaceError::NotReady.into());
        }
        let window = self.parent.stamp()?;
        let (node, renderer, dom_document_id) = self.document.try_with(|document| {
            // Only the actual declared video node is eligible, not an arbitrary div with an id.
            if !document
                .elements()
                .iter()
                .any(|element| element.id == id && element.dom_tag == "video")
            {
                return Err(SurfaceError::NotVideo);
            }
            Ok((
                document.renderer.surface_node(id)?,
                document.renderer.surface_identity(),
                document.renderer.document.id(),
            ))
        })??;
        let reservation = self
            .parent
            .binding
            .surfaces
            .try_borrow_mut()
            .map_err(|_| SurfaceError::Busy)?
            .reserve(node.as_u64())?;
        let paint_owner = Rc::new(());
        let context = Rc::new(BoundSurfaceContext {
            binding: Rc::downgrade(&self.parent.binding),
            coordinator: Rc::downgrade(&self.parent.coordinator),
        });
        self.document.try_with(|document| {
            document.renderer.track_surface(
                context,
                node.as_u64(),
                reservation.generation(),
                Rc::downgrade(&paint_owner),
            )
        })??;
        Ok(NativeSurfaceReservation {
            document: self.document.clone(),
            expected: current,
            window,
            semantic_id: id.into(),
            node,
            renderer,
            dom_document_id,
            registry: Rc::downgrade(&self.parent.binding.surfaces),
            reservation,
            _paint_owner: paint_owner,
        })
    }
    pub fn register_reserved(
        &self,
        reservation: NativeSurfaceReservation,
        create: impl for<'a> FnOnce(
            &NativeScopedParent<'a>,
            NativeParentLease,
            NativeSurfaceSlot,
        ) -> Result<C, ChildCreationFailure<C>>,
    ) -> Result<ChildLease, NativeSurfaceRegistrationFailure<C::Error>> {
        let check: Result<(), NativeSurfaceAccessError> = (|| {
            if self.document.id() != reservation.document.id()
                || !Weak::ptr_eq(
                    &reservation.registry,
                    &Rc::downgrade(&self.parent.binding.surfaces),
                )
            {
                return Err(SurfaceError::Stale.into());
            }
            if self.document.lifecycle() != HostLifecycle::Open
                || self.document.stamp()? != reservation.expected
            {
                return Err(SurfaceError::Stale.into());
            }
            reservation.validate_node()?;
            Ok(())
        })();
        check.map_err(NativeSurfaceRegistrationFailure::Access)?;
        self.parent
            .register(reservation.window, |parent, lease| {
                let slot = NativeSurfaceSlot {
                    access: lease.surface_access(),
                    child: lease.child(),
                    binding: Rc::downgrade(&self.parent.binding),
                    reservation,
                    revoked: Cell::new(false),
                };
                create(parent, lease, slot)
            })
            .map_err(NativeSurfaceRegistrationFailure::Child)
    }
}
impl NativeSurfaceReservation {
    fn validate_node(&self) -> Result<(), NativeSurfaceAccessError> {
        let registry = self.registry.upgrade().ok_or(SurfaceError::Revoked)?;
        registry
            .try_borrow()
            .map_err(|_| SurfaceError::Busy)?
            .validate(&self.reservation)?;
        let current = self.document.stamp()?;
        if current.host_id != self.expected.host_id
            || current.owner != self.expected.owner
            || current.definition_revision != self.expected.definition_revision
        {
            return Err(SurfaceError::Stale.into());
        }
        self.document.try_with(|document| {
            if document.renderer.document.id() != self.dom_document_id
                || !Weak::ptr_eq(&self.renderer, &document.renderer.surface_identity())
                || document.renderer.surface_node(&self.semantic_id)? != self.node
            {
                return Err(SurfaceError::Stale);
            }
            Ok(())
        })??;
        Ok(())
    }
}
/// Same private parent/child authority, but never a clone of the engine's NativeParentLease.
pub struct NativeSurfaceSlot {
    access: Rc<dyn ParentAccess>,
    child: u64,
    binding: Weak<super::parent::ParentBinding>,
    reservation: NativeSurfaceReservation,
    revoked: Cell<bool>,
}
impl NativeSurfaceSlot {
    /// Borrow checked client-coordinate evidence for this original slot and successful frame.
    /// Focus remains independently unverified; this does not enable native child input.
    /// ```compile_fail
    /// use meari_native::infrastructure::window::{NativeSurfaceSlot, NativeCheckedParentClient};
    /// fn escape<'a>(slot: &'a NativeSurfaceSlot) -> &'a NativeCheckedParentClient<'a> {
    ///     slot.with_parent_client(|checked| checked).unwrap()
    /// }
    /// ```
    pub fn with_parent_client<R>(
        &self,
        read: impl for<'a> FnOnce(&super::NativeCheckedParentClient<'a>) -> R,
    ) -> Result<R, super::NativeParentClientError> {
        self.reservation
            .document
            .try_with(|_| {
                let mut expected = Some(self.parent_client_state()?);
                let current = || {
                    self.parent_client_state()
                        .map_err(super::NativeParentClientError::from)
                };
                let mut read = Some(read);
                let mut result = None;
                self.access
                    .visit(None, &mut |parent| {
                        result = Some(super::parent_client::NativeCheckedParentClient::visit(
                            parent,
                            expected.take().expect("one parent visit"),
                            &current,
                            read.take().expect("one checked client visit"),
                        ));
                    })
                    .map_err(NativeSurfaceAccessError::Parent)?;
                result.ok_or(NativeSurfaceAccessError::Slot(SurfaceError::Revoked))?
            })
            .map_err(NativeSurfaceAccessError::Document)?
    }
    fn parent_client_state(
        &self,
    ) -> Result<super::parent_client::ClientState, NativeSurfaceAccessError> {
        if self.revoked.get() || self.reservation.document.lifecycle() != HostLifecycle::Open {
            return Err(SurfaceError::Revoked.into());
        }
        self.reservation.validate_node()?;
        self.reservation.document.try_with(
            |document| -> Result<super::parent_client::ClientState, NativeSurfaceAccessError> {
                let snapshot = self.snapshot(document)?;
                let paint = document
                    .renderer
                    .committed_surface_paint(&snapshot.document)?;
                let placement = paint.placement.ok_or(SurfaceError::Unsupported(
                    "renderer client placement is unverified",
                ))?;
                placement.validate()?;
                if placement.viewport != document.renderer.document.viewport().window_size {
                    return Err(SurfaceError::Stale.into());
                }
                Ok(super::parent_client::ClientState {
                    snapshot,
                    placement,
                })
            },
        )?
    }
    /// Blocks new use first. Original cleanup authority remains available for the retained owner.
    pub fn revoke(&self) {
        self.revoked.set(true);
    }
    pub fn with_surface<R>(
        &self,
        read: impl for<'a> FnOnce(&NativeScopedParent<'a>, &NativeSurfaceSnapshot) -> R,
    ) -> Result<R, NativeSurfaceAccessError> {
        if self.revoked.get() || self.reservation.document.lifecycle() != HostLifecycle::Open {
            return Err(SurfaceError::Revoked.into());
        }
        self.reservation.validate_node()?;
        self.reservation.document.try_with(|document| {
            let snapshot = self.snapshot(document)?;
            let mut read = Some(read);
            let mut result = None;
            self.access.visit(None, &mut |parent| {
                result = Some(read.take().expect("one surface visit")(parent, &snapshot));
            })?;
            result.ok_or_else(|| SurfaceError::Revoked.into())
        })?
    }
    /// Destruction may still require the original parent after the source node was removed/HMR'd.
    /// This checks the original full CleanupRequest; it never remaps to a new source node or ACKs OS work.
    pub fn with_cleanup_surface<R>(
        &self,
        request: CleanupRequest,
        read: impl for<'a> FnOnce(&NativeScopedParent<'a>) -> R,
    ) -> Result<R, LifecycleError> {
        let mut read = Some(read);
        let mut result = None;
        self.access.visit(Some(request), &mut |parent| {
            // The original ParentAccess validates the full request before invoking this closure.
            // Revoke before consumer OS work, but never as a side effect of rejected authority.
            self.revoke();
            result = Some(read.take().expect("one cleanup visit")(parent));
        })?;
        result.ok_or(LifecycleError::WrongState)
    }
    fn snapshot(
        &self,
        document: &InteractiveDocument,
    ) -> Result<NativeSurfaceSnapshot, NativeSurfaceAccessError> {
        let binding = self.binding.upgrade().ok_or(SurfaceError::Revoked)?;
        let geometry = binding.geometry()?.ok_or(SurfaceError::NotReady)?;
        let environment = &geometry.environment;
        let zoom = environment.layout_zoom.ok_or(SurfaceError::NotReady)?;
        let viewport = document.renderer.document.viewport();
        let expected_scale = environment.device_scale * zoom;
        let expected_size = (
            (f64::from(environment.layout.width) * expected_scale).round(),
            (f64::from(environment.layout.height) * expected_scale).round(),
        );
        if (f64::from(viewport.scale()) - expected_scale).abs() > 0.00001
            || expected_size
                != (
                    f64::from(viewport.window_size.0),
                    f64::from(viewport.window_size.1),
                )
        {
            return Err(SurfaceError::NotReady.into());
        }
        let current = self.reservation.document.stamp()?;
        let observed = document.renderer.committed_surface_paint(&current)?;
        if observed.scope.window != self.reservation.window
            || observed.scope.geometry_revision != geometry.revision
        {
            return Err(SurfaceError::Stale.into());
        }
        let target = observed
            .trace
            .targets
            .get(&self.reservation.node.as_u64())
            .ok_or(SurfaceError::Stale)?;
        if target.generation != self.reservation.reservation.generation() {
            return Err(SurfaceError::Stale.into());
        }
        let rect = target.physical_content_rect.map(|rect| PhysicalClientRect {
            x: rect.x0,
            y: rect.y0,
            width: rect.width(),
            height: rect.height(),
        });
        if let Some(rect) = rect {
            rect.validate()?;
        }
        let clip = if target.unsupported {
            SurfaceClip::Unsupported
        } else {
            target
                .rectangular_clip
                .map_or(SurfaceClip::Unknown, |rect| {
                    SurfaceClip::Rectangular(PhysicalClientRect {
                        x: rect.x0,
                        y: rect.y0,
                        width: rect.width(),
                        height: rect.height(),
                    })
                })
        };
        let plane = if target.unsupported || target.later_overlap == blitz_paint::Overlap::Overlap {
            SurfacePlane::Unsupported
        } else if target.later_overlap == blitz_paint::Overlap::Clear
            && matches!(clip, SurfaceClip::Rectangular(_))
        {
            SurfacePlane::RectangularChildWindow
        } else {
            SurfacePlane::Unknown
        };
        let visibility = if !target.painted {
            SurfaceVisibility::Hidden
        } else if target.unsupported
            || target.later_overlap != blitz_paint::Overlap::Clear
            || !matches!(clip, SurfaceClip::Rectangular(_))
        {
            SurfaceVisibility::Unknown
        } else {
            SurfaceVisibility::Visible
        };
        let registry = self
            .reservation
            .registry
            .upgrade()
            .ok_or(SurfaceError::Revoked)?;
        let revision = registry
            .try_borrow_mut()
            .map_err(|_| SurfaceError::Busy)?
            .next_layout(&self.reservation.reservation)?;
        Ok(NativeSurfaceSnapshot {
            frame: observed.frame,
            logical_focus: SurfaceLogicalFocus {
                focused_node: observed.trace.focused_node,
                surface_focusable: target.focusable,
            },
            document: self.reservation.document.stamp()?,
            window: self.reservation.window,
            slot_generation: self.reservation.reservation.generation(),
            layout_revision: revision,
            host_geometry_revision: geometry.revision,
            physical_client_rect: rect,
            device_scale: environment.device_scale,
            layout_zoom: zoom,
            visibility,
            clip,
            plane,
            focus: SurfaceFocus::Unknown,
        })
    }
    pub fn child(&self) -> u64 {
        self.child
    }
    /// Read-only provenance for factory-time pairing. This is not live parent access or readiness.
    /// The original window and child still require post-registration `with_surface` validation.
    pub fn window(&self) -> WindowStamp {
        self.reservation.window
    }
}
#[cfg(test)]
#[path = "tests.rs"]
mod tests;

//! Observation-only window lifetime for hosts without native children; no fake child/coordinator.
use super::{lifecycle::WindowLifecyclePort, parent::*};
use meari::domain::host::HostEnvironment;
use std::{cell::Cell, rc::Rc, sync::Arc, time::Instant};
use winit::window::Window;
pub(super) struct PlainLifecycle {
    pub binding: Rc<ParentBinding>,
    ready: Cell<bool>,
    failed: Cell<bool>,
    closing: Cell<bool>,
}
impl PlainLifecycle {
    pub fn new(binding: Rc<ParentBinding>) -> Self {
        Self {
            binding,
            ready: Cell::new(false),
            failed: Cell::new(false),
            closing: Cell::new(false),
        }
    }
    fn notice(&self, kind: NativeWindowNoticeKind) -> Result<(), String> {
        self.binding
            .publish(kind, None)
            .map_err(|error| error.to_string())
    }
}
impl WindowLifecyclePort for PlainLifecycle {
    fn created(&self, window: Arc<dyn Window>) -> Result<(), String> {
        self.binding
            .created(window)
            .map_err(|error| error.to_string())?;
        self.notice(NativeWindowNoticeKind::Created)
    }
    fn renderer_ready(&self) -> Result<(), String> {
        if self.failed.get() || self.closing.get() || self.ready.replace(true) {
            return Ok(());
        }
        self.notice(NativeWindowNoticeKind::Ready)
    }
    fn submitted(&self, frame: NativeFrameObservation) -> Result<(), String> {
        self.binding
            .submitted(frame, None)
            .map_err(|error| error.to_string())
    }
    fn geometry(&self, environment: HostEnvironment) -> Result<(), String> {
        if self.binding.observe_geometry(environment)? {
            self.notice(NativeWindowNoticeKind::GeometryChanged)?;
        }
        Ok(())
    }
    fn suspend(&self) -> Result<(), String> {
        self.ready.set(false);
        self.binding.pause();
        self.notice(NativeWindowNoticeKind::Suspended)
    }
    fn poll(&self) -> Result<(), String> {
        self.binding
            .seed
            .take(self.binding.host)
            .map_err(|error| error.to_string())?;
        match self.binding.flush_notices() {
            Ok(()) | Err(NativeNoticeError::Paused) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
    fn next_deadline(&self) -> Result<Option<Instant>, String> {
        Ok(None)
    }
    fn forced(&self) -> Result<(), String> {
        self.binding.revoke();
        self.ready.set(false);
        self.notice(NativeWindowNoticeKind::ForcedIncomplete)
    }
    fn failed(&self) -> Result<(), String> {
        self.failed.set(true);
        self.ready.set(false);
        self.notice(NativeWindowNoticeKind::Failed)
    }
    fn closing(&self) -> Result<(), String> {
        self.ready.set(false);
        if !self.closing.replace(true) {
            self.notice(NativeWindowNoticeKind::Revoking)?;
        }
        Ok(())
    }
    fn removed(&self, forced: bool) -> Result<(), String> {
        self.binding.revoke();
        let notice = self.notice(if forced {
            NativeWindowNoticeKind::ForcedIncomplete
        } else {
            NativeWindowNoticeKind::Closed
        });
        self.binding.discard_observer();
        notice
    }
    fn abandon(&self) {
        self.binding.revoke();
        self.binding.discard_observer();
        self.ready.set(false);
    }
}

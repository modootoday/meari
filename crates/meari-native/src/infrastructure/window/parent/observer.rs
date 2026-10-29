//! Split from the parent module to keep files under the 600-line structure limit.
use super::super::forwarding::ForwardingSeed;
use super::*;
use meari::domain::host::HostEnvironment;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    sync::Arc,
};
use winit::{
    raw_window_handle::{HandleError, HasWindowHandle, WindowHandle},
    window::Window,
};

impl fmt::Display for NativeNoticeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native window notice: {self:?}")
    }
}

impl std::error::Error for NativeNoticeError {}

impl Drop for FlushGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

impl ParentBinding {
    pub fn new(
        host: u64,
        seed: Arc<ForwardingSeed>,
        observer: Option<NativeWindowObserver>,
    ) -> Rc<Self> {
        Rc::new(Self {
            surfaces: Rc::new(RefCell::new(
                crate::application::surface_slot::SurfaceRegistry::default(),
            )),
            host,
            seed,
            state: RefCell::new(ParentState {
                window: None,
                epoch: 0,
                ready: false,
                dead: false,
                geometry: None,
                geometry_bytes: Vec::new(),
                notice_sequence: 0,
                notices: VecDeque::new(),
                frame: None,
            }),
            observer: RefCell::new(observer),
            flushing: Cell::new(false),
            observer_failed: Cell::new(false),
            observer_discarded: Cell::new(false),
        })
    }

    pub fn created(&self, window: Arc<dyn Window>) -> Result<(), LifecycleError> {
        self.seed.require_bound(self.host)?;
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| LifecycleError::Busy)?;
        if state.dead {
            return Err(LifecycleError::Closed);
        }
        if state.window.is_some() {
            return Err(LifecycleError::WrongState);
        }
        state.window = Some(window);
        Ok(())
    }

    pub fn mark_ready(&self, stamp: WindowStamp) -> Result<(), LifecycleError> {
        self.seed.require_bound(self.host)?;
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| LifecycleError::Busy)?;
        if state.dead {
            return Err(LifecycleError::Closed);
        }
        if state.window.is_none() {
            return Err(LifecycleError::NotReady);
        }
        state.epoch = stamp.epoch();
        state.ready = true;
        Ok(())
    }

    pub fn pause(&self) {
        if let Ok(mut state) = self.state.try_borrow_mut() {
            state.ready = false;
        }
    }

    pub fn observe_geometry(&self, environment: HostEnvironment) -> Result<bool, String> {
        environment.validate()?;
        let bytes = serde_json::to_vec(&environment).map_err(|error| error.to_string())?;
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| "native geometry is borrowed")?;
        if state.dead {
            return Err("native parent is revoked".into());
        }
        if state.geometry_bytes == bytes {
            return Ok(false);
        }
        let revision = state.geometry.as_ref().map_or(Ok(1), |last| {
            last.revision
                .checked_add(1)
                .ok_or("geometry revision exhausted")
        })?;
        state.geometry = Some(NativeWindowGeometry {
            revision,
            environment,
        });
        state.geometry_bytes = bytes;
        Ok(true)
    }

    pub fn geometry(&self) -> Result<Option<NativeWindowGeometry>, LifecycleError> {
        Ok(self
            .state
            .try_borrow()
            .map_err(|_| LifecycleError::Busy)?
            .geometry
            .clone())
    }

    pub fn window(&self) -> Result<Arc<dyn Window>, LifecycleError> {
        let state = self.state.try_borrow().map_err(|_| LifecycleError::Busy)?;
        if state.dead {
            return Err(LifecycleError::Closed);
        }
        state.window.clone().ok_or(LifecycleError::NotReady)
    }

    pub(super) fn check(&self, epoch: u64, ready: bool) -> Result<(), LifecycleError> {
        self.seed.require_bound(self.host)?;
        let state = self.state.try_borrow().map_err(|_| LifecycleError::Busy)?;
        if state.dead {
            return Err(LifecycleError::Closed);
        }
        if state.window.is_none() {
            return Err(LifecycleError::NotReady);
        }
        if state.epoch != epoch {
            return Err(LifecycleError::Stale);
        }
        if ready && !state.ready {
            return Err(LifecycleError::Suspended);
        }
        Ok(())
    }

    pub fn publish(
        &self,
        kind: NativeWindowNoticeKind,
        window: Option<WindowStamp>,
    ) -> Result<(), NativeNoticeError> {
        {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| NativeNoticeError::Busy)?;
            if state.dead
                && !matches!(
                    kind,
                    NativeWindowNoticeKind::Closed
                        | NativeWindowNoticeKind::ForcedIncomplete
                        | NativeWindowNoticeKind::Failed
                )
            {
                return Err(NativeNoticeError::Revoked);
            }
            let revision = state
                .notice_sequence
                .checked_add(1)
                .ok_or(NativeNoticeError::CounterExhausted)?;
            state.notice_sequence = revision;
            // At most one latest notice per enum kind (ten); no unbounded event or payload queue.
            state.notices.retain(|notice| notice.kind != kind);
            let geometry = state.geometry.clone();
            let frame = if kind == NativeWindowNoticeKind::FrameSubmitted {
                state.frame.clone()
            } else {
                None
            };
            state.notices.push_back(NativeWindowNotice {
                revision,
                host_id: self.host,
                kind,
                window,
                geometry,
                frame,
            });
        }
        if self.flushing.get() {
            let _ = self.seed.notify();
            return Err(NativeNoticeError::Busy);
        }
        self.flush_notices()
    }

    pub fn flush_notices(&self) -> Result<(), NativeNoticeError> {
        if self.observer_failed.get() {
            return Err(NativeNoticeError::Paused);
        }
        if self.flushing.replace(true) {
            return Err(NativeNoticeError::Busy);
        }
        let _flush = FlushGuard(&self.flushing);
        let observer = self
            .observer
            .try_borrow_mut()
            .map_err(|_| NativeNoticeError::Busy)?
            .take();
        let Some(mut observer) = observer else {
            return Ok(());
        };
        // One bounded batch only. Reentrant updates remain for the next native wake.
        let batch = self
            .state
            .borrow()
            .notices
            .iter()
            .map(|notice| notice.revision)
            .collect::<Vec<_>>();
        let mut failure = None;
        for revision in batch {
            let notice = self
                .state
                .borrow()
                .notices
                .iter()
                .find(|notice| notice.revision == revision)
                .cloned();
            let Some(notice) = notice else {
                continue;
            };
            match catch_unwind(AssertUnwindSafe(|| observer(notice))) {
                Ok(Ok(())) => self
                    .state
                    .borrow_mut()
                    .notices
                    .retain(|notice| notice.revision != revision),
                Ok(Err(error)) => {
                    failure = Some(NativeNoticeError::Callback(error));
                    break;
                }
                Err(_) => {
                    failure = Some(NativeNoticeError::Panicked);
                    break;
                }
            }
        }
        if !self.observer_discarded.get() {
            *self.observer.borrow_mut() = Some(observer);
        }
        if let Some(error) = failure {
            self.observer_failed.set(true);
            return Err(error);
        }
        Ok(())
    }

    pub fn take_notices(&self) -> Result<Vec<NativeWindowNotice>, NativeNoticeError> {
        if self.flushing.get() {
            return Err(NativeNoticeError::Busy);
        }
        Ok(self
            .state
            .try_borrow_mut()
            .map_err(|_| NativeNoticeError::Busy)?
            .notices
            .drain(..)
            .collect())
    }

    pub fn retry_observer(&self) -> Result<(), NativeNoticeError> {
        if self.observer_discarded.get() {
            return Err(NativeNoticeError::Revoked);
        }
        if self.flushing.get() {
            return Err(NativeNoticeError::Busy);
        }
        self.observer_failed.set(false);
        self.flush_notices()
    }

    /// Revoke immediately, then drop native ownership outside the RefCell guard. Never run observers.
    pub fn revoke(&self) {
        self.seed.close();
        let window = {
            let mut state = self.state.borrow_mut();
            state.dead = true;
            state.ready = false;
            state.frame = None;
            state.window.take()
        };
        drop(window);
    }

    pub fn discard_observer(&self) {
        self.observer_discarded.set(true);
        let observer = self.observer.borrow_mut().take();
        drop(observer);
    }

    pub fn submitted(
        &self,
        frame: NativeFrameObservation,
        window: Option<WindowStamp>,
    ) -> Result<(), NativeNoticeError> {
        {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| NativeNoticeError::Busy)?;
            if state.dead {
                return Err(NativeNoticeError::Revoked);
            }
            state.frame = Some(frame);
        }
        self.publish(NativeWindowNoticeKind::FrameSubmitted, window)
    }
}

impl HasWindowHandle for NativeScopedParent<'_> {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        (self.validate)().map_err(|_| HandleError::Unavailable)?;
        self.window.window_handle()
    }
}

impl<C: NativeChild> ParentAccess for ChildParentAccess<C> {
    fn visit(
        &self,
        cleanup: Option<CleanupRequest>,
        read: &mut dyn FnMut(&NativeScopedParent<'_>),
    ) -> Result<(), LifecycleError> {
        let binding = self.binding.upgrade().ok_or(LifecycleError::Closed)?;
        let coordinator = self.coordinator.upgrade().ok_or(LifecycleError::Closed)?;
        let validate = || {
            if let Some(request) = cleanup {
                coordinator.validate_parent_cleanup(&self.authority, request)?;
            } else {
                coordinator.validate_access(&self.authority)?;
            }
            binding.check(self.authority.window().epoch(), cleanup.is_none())
        };
        validate()?;
        let window = binding.window()?;
        read(&NativeScopedParent {
            window: window.as_ref(),
            validate: &validate,
        });
        Ok(())
    }
}

impl<C: NativeChild> Clone for NativeChildRegistrar<C> {
    fn clone(&self) -> Self {
        Self {
            binding: self.binding.clone(),
            coordinator: self.coordinator.clone(),
        }
    }
}

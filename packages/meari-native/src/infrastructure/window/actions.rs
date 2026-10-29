//! Same-loop adapter. The public port is weak and UI-thread-only; it never exports a Window.
use super::{Application, NativeWindowWake, access};
use crate::{
    application::{
        window_actions::*,
        window_lifecycle::{NativeChild, WindowLifecycle},
    },
    domain::window_lifecycle::WindowPhase,
    infrastructure::native_host::{HostLifecycle, NativeDocumentHandle},
};
use std::{
    cell::{Cell, RefCell},
    num::NonZeroU64,
    rc::{Rc, Weak},
    time::Instant,
};
use winit::{
    monitor::Fullscreen,
    window::{Window, WindowId},
};

/// A weak command port issued only by the original prepared window owner.
#[derive(Clone)]
pub struct NativeWindowActions {
    binding: Weak<ActionBinding>,
}
impl NativeWindowActions {
    pub fn input_activity(&self) -> Result<Option<WindowInputActivity>, ActionError> {
        let binding = self.binding()?;
        let current = binding.current()?;
        let activity = binding
            .activity
            .try_borrow()
            .map_err(|_| ActionError::Busy)?;
        activity.latest(current)
    }
    fn binding(&self) -> Result<Rc<ActionBinding>, ActionError> {
        let binding = self.binding.upgrade().ok_or(ActionError::Closed)?;
        if std::thread::current().id() != binding.thread {
            return Err(ActionError::WrongThread);
        }
        Ok(binding)
    }
    pub fn snapshot(&self) -> Result<ActionSnapshot, ActionError> {
        let binding = self.binding()?;
        let scope = binding.current()?;
        let snapshot = binding
            .model
            .try_borrow()
            .map_err(|_| ActionError::Busy)?
            .snapshot()
            .ok_or(ActionError::NotReady)?;
        if snapshot.scope != scope {
            return Err(ActionError::StaleWindow);
        }
        Ok(snapshot)
    }
    pub fn request(
        &self,
        expected: ActionScope,
        id: NonZeroU64,
        action: WindowAction,
    ) -> Result<ActionAdmission, ActionError> {
        let binding = self.binding()?;
        let current = binding.current()?;
        if expected.host != current.host {
            return Err(ActionError::ForeignHost);
        }
        if expected != current {
            return Err(ActionError::StaleWindow);
        }
        let hide_sequence = if action == WindowAction::SetCursorVisible(false) {
            Some(
                binding
                    .activity
                    .try_borrow()
                    .map_err(|_| ActionError::Busy)?
                    .sequence(),
            )
        } else {
            None
        };
        let admitted = binding
            .model
            .try_borrow_mut()
            .map_err(|_| ActionError::Busy)?
            .admit(expected, id, action)?;
        if !admitted.duplicate {
            if let Some(sequence) = hide_sequence {
                binding.cursor_hide_activity.set(Some((id, sequence)));
            }
            binding.activated.set(true);
            if binding.wake.request_actions().is_err() {
                binding
                    .model
                    .try_borrow_mut()
                    .map_err(|_| ActionError::Busy)?
                    .fail(admitted.record.ticket, ActionError::Closed)?;
                return Err(ActionError::Closed);
            }
        }
        Ok(admitted)
    }
    pub fn record(&self, id: NonZeroU64) -> Result<Option<ActionRecord>, ActionError> {
        let binding = self.binding()?;
        let result = binding
            .model
            .try_borrow()
            .map_err(|_| ActionError::Busy)?
            .record(id);
        Ok(result)
    }
    pub fn take_result(&self, id: NonZeroU64) -> Result<Option<ActionRecord>, ActionError> {
        let binding = self.binding()?;
        let result = binding
            .model
            .try_borrow_mut()
            .map_err(|_| ActionError::Busy)?
            .take_result(id);
        Ok(result)
    }
}

pub(super) struct ActionBinding {
    scope: Box<dyn Fn() -> Result<ActionScope, ActionError>>,
    model: RefCell<WindowActions>,
    wake: NativeWindowWake,
    activated: Cell<bool>,
    origin: Instant,
    thread: std::thread::ThreadId,
    activity: RefCell<super::input::InputActivity>,
    cursor_hidden: Cell<bool>,
    cursor_hide_activity: Cell<Option<(NonZeroU64, u64)>>,
}
impl ActionBinding {
    pub(super) fn new<C: NativeChild>(
        document: NativeDocumentHandle,
        children: Weak<WindowLifecycle<C>>,
        wake: NativeWindowWake,
    ) -> Rc<Self> {
        let host = document.id();
        Rc::new(Self {
            scope: Box::new(move || {
                match document.lifecycle() {
                    HostLifecycle::Open => (),
                    HostLifecycle::Closed | HostLifecycle::ForcedIncomplete => {
                        return Err(ActionError::Closed);
                    }
                    _ => return Err(ActionError::Closing),
                }
                let children = children.upgrade().ok_or(ActionError::Closed)?;
                match children.phase().map_err(|_| ActionError::Busy)? {
                    WindowPhase::Ready => (),
                    WindowPhase::Closing => return Err(ActionError::Closing),
                    WindowPhase::Closed | WindowPhase::ForcedIncomplete => {
                        return Err(ActionError::Closed);
                    }
                    _ => return Err(ActionError::NotReady),
                }
                Ok(ActionScope {
                    host,
                    window: children.stamp().map_err(|_| ActionError::Busy)?,
                })
            }),
            model: RefCell::new(
                WindowActions::new(host, 32).expect("positive original host and bounded ledger"),
            ),
            wake,
            activated: Cell::new(false),
            origin: Instant::now(),
            thread: std::thread::current().id(),
            activity: RefCell::new(super::input::InputActivity::new(host)),
            cursor_hidden: Cell::new(false),
            cursor_hide_activity: Cell::new(None),
        })
    }
    pub(super) fn handle(self: &Rc<Self>) -> NativeWindowActions {
        NativeWindowActions {
            binding: Rc::downgrade(self),
        }
    }
    pub(super) fn current(&self) -> Result<ActionScope, ActionError> {
        if std::thread::current().id() != self.thread {
            return Err(ActionError::WrongThread);
        }
        (self.scope)()
    }
    fn invalidate(&self, error: ActionError) {
        if error == ActionError::Busy {
            return;
        }
        if let Ok(mut model) = self.model.try_borrow_mut() {
            model.invalidate(error);
        }
        if let Ok(mut activity) = self.activity.try_borrow_mut() {
            activity.invalidate(error);
        }
        self.cursor_hide_activity.set(None);
    }
    pub(super) fn restore_cursor(&self, window: &dyn Window) {
        if self.cursor_hidden.replace(false) {
            window.set_cursor_visible(true);
        }
    }
    pub(super) fn retire_input(&self) {
        if let Ok(mut activity) = self.activity.try_borrow_mut() {
            activity.retire();
        }
        self.cursor_hide_activity.set(None);
    }
    pub(super) fn record_input(
        &self,
        window: &dyn Window,
        kind: WindowInputKind,
    ) -> Result<(), ActionError> {
        let scope = match self.current() {
            Ok(scope) => scope,
            Err(ActionError::NotReady) => {
                return self
                    .activity
                    .try_borrow_mut()
                    .map_err(|_| ActionError::Busy)?
                    .record_pending(window.id(), kind, self.origin.elapsed());
            }
            Err(error) => return Err(error),
        };
        // Restoration belongs only to a cursor hidden by this original window port.
        self.restore_cursor(window);
        self.cursor_hide_activity.set(None);
        self.activity
            .try_borrow_mut()
            .map_err(|_| ActionError::Busy)?
            .publish_initial(scope, window.id())?;
        self.activity
            .try_borrow_mut()
            .map_err(|_| ActionError::Busy)?
            .record(scope, kind, self.origin.elapsed())?;
        self.wake.wake().map_err(|_| ActionError::Closed)?;
        Ok(())
    }
    fn drive(&self, window: &dyn Window) -> Result<bool, ActionError> {
        let scope = match self.current() {
            Ok(scope) => scope,
            Err(error) => {
                self.restore_cursor(window);
                self.invalidate(error);
                return Err(error);
            }
        };
        let published = self
            .activity
            .try_borrow_mut()
            .map_err(|_| ActionError::Busy)?
            .publish_initial(scope, window.id())?;
        if published {
            self.wake.wake().map_err(|_| ActionError::Closed)?;
        }
        let (capabilities, observed) = observation(window);
        let mut model = self.model.try_borrow_mut().map_err(|_| ActionError::Busy)?;
        let mut changed = model.observe(scope, capabilities, observed, self.origin.elapsed())?;
        if let Some(command) = model.next(scope)? {
            if command.action == WindowAction::SetCursorVisible(false)
                && self.cursor_hide_activity.get()
                    != Some((
                        command.ticket.id,
                        self.activity
                            .try_borrow()
                            .map_err(|_| ActionError::Busy)?
                            .sequence(),
                    ))
            {
                model.fail(command.ticket, ActionError::StaleRequest)?;
                return Ok(true);
            }
            // Admission is not authority to execute against a replaced/closing window.
            let issued = issue_if_current(
                command,
                || self.current(),
                |action| {
                    match action {
                        WindowAction::SetMinimized(value) => window.set_minimized(value),
                        WindowAction::SetMaximized(value) => window.set_maximized(value),
                        WindowAction::SetFullscreen(value) => {
                            window.set_fullscreen(value.then_some(Fullscreen::Borderless(None)))
                        }
                        WindowAction::SetCursorVisible(value) => {
                            window.set_cursor_visible(value);
                            self.cursor_hidden.set(!value);
                        }
                    }
                    Ok(())
                },
            );
            if let Err(error) = issued {
                model.fail(command.ticket, error)?;
                return Ok(true);
            }
            changed = true;
            let current = match self.current() {
                Ok(current) => current,
                Err(error) => {
                    self.restore_cursor(window);
                    model.invalidate(error);
                    return Ok(true);
                }
            };
            if current != command.ticket.scope {
                self.restore_cursor(window);
                model.fail(command.ticket, ActionError::StaleWindow)?;
                return Ok(true);
            }
            if matches!(command.action, WindowAction::SetCursorVisible(_)) {
                model.applied(command.ticket)?;
                self.cursor_hide_activity.set(None);
            }
            let (capabilities, observed) = observation(window);
            changed |= model.observe(current, capabilities, observed, self.origin.elapsed())?;
        }
        Ok(changed)
    }
}

#[cfg(windows)]
fn observation(window: &dyn Window) -> (ActionCapabilities, WindowObservation) {
    use windows::Win32::{
        Foundation::{HWND, RECT},
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
        },
        UI::WindowsAndMessaging::{
            GetWindowInfo, GetWindowRect, IsIconic, IsWindow, IsZoomed, WINDOWINFO, WS_CAPTION,
            WS_THICKFRAME,
        },
    };
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let unknown = (ActionCapabilities::default(), WindowObservation::default());
    let Ok(handle) = window.window_handle() else {
        return unknown;
    };
    let RawWindowHandle::Win32(raw) = handle.as_raw() else {
        return unknown;
    };
    let hwnd = HWND(raw.hwnd.get() as *mut std::ffi::c_void);
    // Original UI-thread Window borrow only. No OS handle is stored or exported.
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return unknown;
        }
        let mut rect = RECT::default();
        let mut info = WINDOWINFO {
            cbSize: std::mem::size_of::<WINDOWINFO>() as u32,
            ..Default::default()
        };
        let mut monitor = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let borderless_monitor_coverage = if GetWindowRect(hwnd, &mut rect).is_ok()
            && GetWindowInfo(hwnd, &mut info).is_ok()
            && GetMonitorInfoW(
                MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            )
            .as_bool()
        {
            Some(
                info.dwStyle.0 & (WS_CAPTION.0 | WS_THICKFRAME.0) == 0
                    && rect.left == monitor.rcMonitor.left
                    && rect.top == monitor.rcMonitor.top
                    && rect.right == monitor.rcMonitor.right
                    && rect.bottom == monitor.rcMonitor.bottom,
            )
        } else {
            None
        };
        (
            ActionCapabilities {
                minimize: true,
                maximize: true,
                borderless_fullscreen: borderless_monitor_coverage.is_some(),
            },
            WindowObservation {
                minimized: Some(IsIconic(hwnd).as_bool()),
                maximized: Some(IsZoomed(hwnd).as_bool()),
                borderless_monitor_coverage,
            },
        )
    }
}
#[cfg(not(windows))]
fn observation(_window: &dyn Window) -> (ActionCapabilities, WindowObservation) {
    (ActionCapabilities::default(), WindowObservation::default())
}

impl Application {
    pub(super) fn drive_window_actions(&mut self, id: WindowId) {
        let Some(view) = self.inner.windows.get(&id) else {
            return;
        };
        let Some(document) = access::handle(view.doc.as_ref()) else {
            return;
        };
        let Some(binding) = self.actions.get(&document.id()).cloned() else {
            return;
        };
        let result = binding.drive(view.window.as_ref());
        match result {
            Ok(true) if binding.activated.get() => {
                let _ = binding.wake.wake();
            }
            Err(error)
                if !matches!(
                    error,
                    ActionError::NotReady
                        | ActionError::Closing
                        | ActionError::Closed
                        | ActionError::Busy
                ) =>
            {
                self.report(error.to_string());
            }
            _ => (),
        }
    }
    pub(super) fn poll_window_actions(&mut self) {
        for id in self.inner.windows.keys().copied().collect::<Vec<_>>() {
            self.drive_window_actions(id);
        }
        for (host, binding) in &self.actions {
            if self.window_for_host(*host).is_none()
                && let Err(error) = binding.current()
            {
                binding.invalidate(error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::wake;
    #[test]
    fn action_wake_preserves_original_bits_and_send_sync_contract() {
        fn thread_safe<T: Send + Sync>() {}
        thread_safe::<super::NativeWindowWake>();
        assert_eq!(
            (wake::COMPLETE, wake::CLOSE, wake::LIFECYCLE, wake::RETRY),
            (1, 2, 4, 8)
        );
        assert_eq!(
            wake::ACTIONS & (wake::COMPLETE | wake::CLOSE | wake::LIFECYCLE | wake::RETRY),
            0
        );
    }
}

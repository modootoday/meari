use super::*;
use windows::{
    Win32::{
        Foundation::{HWND, RECT},
        UI::WindowsAndMessaging::*,
    },
    core::w,
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[derive(Clone, Copy)]
pub(super) enum Mode {
    Complete,
    Pending,
    FailOnce,
}
pub(super) struct State {
    pub handle: Option<HWND>,
    pub original: Option<HWND>,
    pub parent: Option<HWND>,
    pub starts: usize,
    pub observations: usize,
    pub losses: usize,
    pub terminal: bool,
    pub signal: Option<CompletionSignal>,
    pub lease: Option<Rc<NativeParentLease>>,
    pub mode: Mode,
}
pub(super) fn state(mode: Mode) -> Rc<RefCell<State>> {
    Rc::new(RefCell::new(State {
        handle: None,
        original: None,
        parent: None,
        starts: 0,
        observations: 0,
        losses: 0,
        terminal: false,
        signal: None,
        lease: None,
        mode,
    }))
}
fn hwnd(parent: &NativeScopedParent<'_>) -> io::Result<HWND> {
    match parent.window_handle().map_err(io::Error::other)?.as_raw() {
        RawWindowHandle::Win32(handle) => Ok(HWND(handle.hwnd.get() as *mut std::ffi::c_void)),
        _ => Err(io::Error::other("expected a Windows native parent")),
    }
}
pub(super) struct Child {
    state: Rc<RefCell<State>>,
    parent: Rc<NativeParentLease>,
}
impl Child {
    pub fn create(
        parent: &NativeScopedParent<'_>,
        lease: NativeParentLease,
        state: Rc<RefCell<State>>,
    ) -> Result<Self, ChildCreationFailure<Self>> {
        let created = (|| -> io::Result<(HWND, HWND)> {
            let parent = hwnd(parent)?;
            // Built-in STATIC control: no custom wndproc, SDK, media engine or new dependency.
            let child = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("STATIC"),
                    w!("Meari technical child"),
                    WS_CHILD | WS_VISIBLE,
                    0,
                    0,
                    64,
                    40,
                    Some(parent),
                    None,
                    None,
                    None,
                )
            }
            .map_err(io::Error::other)?;
            Ok((parent, child))
        })();
        let (parent_handle, handle) = created.map_err(|error| ChildCreationFailure {
            error,
            partial: None,
        })?;
        let parent = Rc::new(lease);
        {
            let mut value = state.borrow_mut();
            value.handle = Some(handle);
            value.original = Some(handle);
            value.parent = Some(parent_handle);
            value.lease = Some(parent.clone());
        }
        let child = Self { state, parent };
        match unsafe { GetParent(handle) } {
            Ok(observed) if observed == parent_handle => Ok(child),
            other => Err(ChildCreationFailure {
                error: io::Error::other(format!("actual child parent mismatch: {other:?}")),
                partial: Some(child),
            }),
        }
    }
    fn cleanup_parent(&self, request: CleanupRequest) -> io::Result<()> {
        self.parent
            .with_cleanup_parent(request, hwnd)
            .map_err(io::Error::other)??;
        Ok(())
    }
    fn destroy(&self) -> io::Result<CleanupProgress> {
        let handle = self.state.borrow().handle;
        if let Some(handle) = handle {
            unsafe { DestroyWindow(handle) }.map_err(io::Error::other)?;
        }
        self.state.borrow_mut().handle = None;
        Ok(CleanupProgress::Complete)
    }
}
impl NativeChild for Child {
    type Error = io::Error;
    fn parent_lost(&mut self) {
        self.state.borrow_mut().losses += 1;
    }
    fn begin_cleanup(
        &mut self,
        request: CleanupRequest,
        signal: CompletionSignal,
    ) -> io::Result<CleanupProgress> {
        self.cleanup_parent(request)?;
        let (mode, attempt) = {
            let mut state = self.state.borrow_mut();
            state.starts += 1;
            state.signal = Some(signal);
            (state.mode, state.starts)
        };
        match mode {
            Mode::Pending => Ok(CleanupProgress::Pending),
            Mode::FailOnce if attempt == 1 => {
                Err(io::Error::other("injected terminal child failure"))
            }
            _ => self.destroy(),
        }
    }
    fn observe_cleanup(&mut self, request: CleanupRequest) -> io::Result<CleanupProgress> {
        self.cleanup_parent(request)?;
        let terminal = {
            let mut state = self.state.borrow_mut();
            state.observations += 1;
            state.terminal
        };
        if terminal {
            self.destroy()
        } else {
            Ok(CleanupProgress::Pending)
        }
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        // Technical adapter RAII is not a coordinator cleanup ACK.
        if let Some(handle) = self.state.borrow_mut().handle.take() {
            let _ = unsafe { DestroyWindow(handle) };
        }
    }
}
pub(super) fn is_window(handle: HWND) -> bool {
    unsafe { IsWindow(Some(handle)).as_bool() }
}
pub(super) fn resize_parent(state: &Rc<RefCell<State>>) {
    let parent = state.borrow().parent.unwrap();
    let mut bounds = RECT::default();
    unsafe { GetWindowRect(parent, &mut bounds) }.unwrap();
    unsafe {
        MoveWindow(
            parent,
            bounds.left,
            bounds.top,
            bounds.right - bounds.left + 80,
            bounds.bottom - bounds.top + 60,
            true,
        )
    }
    .unwrap();
}
pub(super) fn match_child_bounds(state: &Rc<RefCell<State>>) {
    let state = state.borrow();
    let mut parent_bounds = RECT::default();
    let mut child_bounds = RECT::default();
    unsafe { GetClientRect(state.parent.unwrap(), &mut parent_bounds) }.unwrap();
    unsafe {
        MoveWindow(
            state.handle.unwrap(),
            0,
            0,
            parent_bounds.right,
            parent_bounds.bottom,
            true,
        )
    }
    .unwrap();
    unsafe { GetClientRect(state.handle.unwrap(), &mut child_bounds) }.unwrap();
    assert_eq!(
        (child_bounds.right, child_bounds.bottom),
        (parent_bounds.right, parent_bounds.bottom)
    );
}

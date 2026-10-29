//! Callback-scoped client-coordinate evidence. Never an OS presentation/focus acknowledgement.
use super::{NativeScopedParent, NativeSurfaceAccessError, NativeSurfaceSnapshot};
use crate::infrastructure::engine::surface_slots::RendererClientPlacement;
use std::{marker::PhantomData, rc::Rc};

#[derive(Debug)]
pub enum NativeParentClientError {
    Access(NativeSurfaceAccessError),
    UnsupportedPlatform,
    OsReadback(String),
    PlacementMismatch,
}
impl std::fmt::Display for NativeParentClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native parent client: {self:?}")
    }
}
impl std::error::Error for NativeParentClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Access(error) => Some(error),
            _ => None,
        }
    }
}
impl From<NativeSurfaceAccessError> for NativeParentClientError {
    fn from(error: NativeSurfaceAccessError) -> Self {
        Self::Access(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ClientReadback {
    pub size: (u32, u32),
    pub screen_origin: (i32, i32),
}
pub(super) struct ClientState {
    pub snapshot: NativeSurfaceSnapshot,
    pub placement: RendererClientPlacement,
}
impl ClientState {
    fn same_frame(&self, current: &Self) -> bool {
        let a = &self.snapshot;
        let b = &current.snapshot;
        a.document == b.document
            && a.window == b.window
            && a.frame == b.frame
            && a.slot_generation == b.slot_generation
            && a.host_geometry_revision == b.host_geometry_revision
            && a.physical_client_rect == b.physical_client_rect
            && a.device_scale == b.device_scale
            && a.layout_zoom == b.layout_zoom
            && a.visibility == b.visibility
            && a.clip == b.clip
            && a.plane == b.plane
            && a.focus == b.focus
            && a.logical_focus == b.logical_focus
            && self.placement == current.placement
    }
}

/// Opaque, UI-thread borrow of one checked parent and successful frame.
/// Extracted snapshots and raw handles remain observations, never reusable client authority.
/// ```compile_fail
/// use meari_native::infrastructure::window::NativeCheckedParentClient;
/// fn construct() { let _ = NativeCheckedParentClient {}; }
/// ```
/// ```compile_fail
/// use meari_native::infrastructure::window::NativeCheckedParentClient;
/// fn duplicate(value: &NativeCheckedParentClient<'_>) { let _: NativeCheckedParentClient<'_> = value.clone(); }
/// ```
/// ```compile_fail
/// use meari_native::infrastructure::window::NativeCheckedParentClient;
/// fn send<T: Send>() {} fn check() { send::<NativeCheckedParentClient<'static>>(); }
/// ```
/// ```compile_fail
/// use meari_native::infrastructure::window::NativeCheckedParentClient;
/// fn sync<T: Sync>() {} fn check() { sync::<NativeCheckedParentClient<'static>>(); }
/// ```
/// ```compile_fail
/// use meari_native::infrastructure::window::NativeCheckedParentClient;
/// fn serialize(value: &NativeCheckedParentClient<'_>) { let _ = serde_json::to_string(value); }
/// ```
/// ```compile_fail
/// use meari_native::infrastructure::window::NativeCheckedParentClient;
/// fn deserialize() { let _: NativeCheckedParentClient<'static> = serde_json::from_str("{}").unwrap(); }
/// ```
pub struct NativeCheckedParentClient<'a> {
    parent: &'a NativeScopedParent<'a>,
    expected: ClientState,
    observed: ClientReadback,
    current: &'a dyn Fn() -> Result<ClientState, NativeParentClientError>,
    _ui_thread: PhantomData<Rc<()>>,
}
impl<'a> NativeCheckedParentClient<'a> {
    pub(super) fn visit<R>(
        parent: &'a NativeScopedParent<'a>,
        expected: ClientState,
        current: &'a dyn Fn() -> Result<ClientState, NativeParentClientError>,
        read: impl for<'b> FnOnce(&NativeCheckedParentClient<'b>) -> R,
    ) -> Result<R, NativeParentClientError> {
        let observed = validate_observation(&expected, None, current, &|| read_client(parent))?;
        let checked = Self {
            parent,
            expected,
            observed,
            current,
            _ui_thread: PhantomData,
        };
        Ok(read(&checked))
    }
    /// Recheck the original frame, lease and OS coordinates immediately before use.
    pub fn with_parent<R>(
        &self,
        read: impl for<'b> FnOnce(&NativeScopedParent<'b>, &NativeSurfaceSnapshot) -> R,
    ) -> Result<R, NativeParentClientError> {
        validate_observation(&self.expected, Some(self.observed), self.current, &|| {
            read_client(self.parent)
        })?;
        Ok(read(self.parent, &self.expected.snapshot))
    }
}

fn validate_observation(
    expected: &ClientState,
    previous: Option<ClientReadback>,
    current: &dyn Fn() -> Result<ClientState, NativeParentClientError>,
    read: &dyn Fn() -> Result<ClientReadback, NativeParentClientError>,
) -> Result<ClientReadback, NativeParentClientError> {
    expected
        .placement
        .validate()
        .map_err(NativeSurfaceAccessError::Slot)?;
    let stale = || NativeSurfaceAccessError::Slot(crate::domain::surface_slot::SurfaceError::Stale);
    if !expected.same_frame(&current()?) {
        return Err(stale().into());
    }
    let first = read()?;
    if first.size != expected.placement.target || previous.is_some_and(|last| last != first) {
        return Err(NativeParentClientError::PlacementMismatch);
    }
    if !expected.same_frame(&current()?) {
        return Err(stale().into());
    }
    if read()? != first {
        return Err(NativeParentClientError::PlacementMismatch);
    }
    if !expected.same_frame(&current()?) {
        return Err(stale().into());
    }
    Ok(first)
}

fn read_client(parent: &NativeScopedParent<'_>) -> Result<ClientReadback, NativeParentClientError> {
    parent
        .validate_live()
        .map_err(NativeSurfaceAccessError::Parent)?;
    #[cfg(windows)]
    {
        use windows::Win32::{
            Foundation::{HWND, POINT, RECT},
            Graphics::Gdi::ClientToScreen,
            UI::WindowsAndMessaging::GetClientRect,
        };
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let raw = parent
            .window_handle()
            .map_err(|e| NativeParentClientError::OsReadback(e.to_string()))?;
        let RawWindowHandle::Win32(raw) = raw.as_raw() else {
            return Err(NativeParentClientError::UnsupportedPlatform);
        };
        let hwnd = HWND(raw.hwnd.get() as *mut std::ffi::c_void);
        let mut rect = RECT::default();
        let mut origin = POINT::default();
        // The parent borrow keeps the original window owner alive. No retained raw handle.
        unsafe {
            GetClientRect(hwnd, &mut rect)
                .map_err(|e| NativeParentClientError::OsReadback(e.to_string()))?;
            ClientToScreen(hwnd, &mut origin)
                .ok()
                .map_err(|e| NativeParentClientError::OsReadback(e.to_string()))?;
        }
        parent
            .validate_live()
            .map_err(NativeSurfaceAccessError::Parent)?;
        let width = rect
            .right
            .checked_sub(rect.left)
            .and_then(|v| u32::try_from(v).ok());
        let height = rect
            .bottom
            .checked_sub(rect.top)
            .and_then(|v| u32::try_from(v).ok());
        match (width, height) {
            (Some(width), Some(height))
                if rect.left == 0 && rect.top == 0 && width > 0 && height > 0 =>
            {
                Ok(ClientReadback {
                    size: (width, height),
                    screen_origin: (origin.x, origin.y),
                })
            }
            _ => Err(NativeParentClientError::PlacementMismatch),
        }
    }
    #[cfg(not(windows))]
    {
        Err(NativeParentClientError::UnsupportedPlatform)
    }
}

#[cfg(test)]
mod tests;

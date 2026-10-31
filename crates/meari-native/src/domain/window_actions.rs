//! Window intent and observations. Accepted/issued is not OS presentation acknowledgement.
use super::window_lifecycle::WindowStamp;
use std::{num::NonZeroU64, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionScope {
    pub host: u64,
    pub window: WindowStamp,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowAction {
    SetMinimized(bool),
    SetMaximized(bool),
    /// Borderless coverage of the current monitor, never exclusive video-mode switching.
    SetFullscreen(bool),
    /// Original window-local setter. Applied does not prove OS cursor visibility.
    SetCursorVisible(bool),
    /// Starts an OS-driven interactive resize from the given edge, for apps that draw their own
    /// title bar and borders. Call while the primary button is held (e.g. on pointer down).
    /// Applied means the OS accepted the request; the resize itself is not observed.
    BeginResize(ResizeDirection),
}
/// Window edge or corner an interactive resize starts from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeDirection {
    East,
    North,
    NorthEast,
    NorthWest,
    South,
    SouthEast,
    SouthWest,
    West,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActionCapabilities {
    pub minimize: bool,
    pub maximize: bool,
    pub borderless_fullscreen: bool,
}
impl ActionCapabilities {
    pub fn supports(self, action: WindowAction) -> bool {
        match action {
            WindowAction::SetMinimized(_) => self.minimize,
            WindowAction::SetMaximized(_) => self.maximize,
            WindowAction::SetFullscreen(_) => self.borderless_fullscreen,
            WindowAction::SetCursorVisible(_) | WindowAction::BeginResize(_) => true,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowObservation {
    pub minimized: Option<bool>,
    pub maximized: Option<bool>,
    pub borderless_monitor_coverage: Option<bool>,
}
impl WindowObservation {
    pub fn matches(self, action: WindowAction) -> bool {
        match action {
            WindowAction::SetMinimized(value) => self.minimized == Some(value),
            WindowAction::SetMaximized(value) => self.maximized == Some(value),
            WindowAction::SetFullscreen(value) => self.borderless_monitor_coverage == Some(value),
            WindowAction::SetCursorVisible(_) | WindowAction::BeginResize(_) => false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionError {
    NotReady,
    Closing,
    Closed,
    ForeignHost,
    StaleWindow,
    Unsupported,
    Capacity,
    StaleRequest,
    ConflictingDuplicate,
    Busy,
    CounterExhausted,
    BackendFailed,
    StaleObservation,
    WrongThread,
}
impl std::fmt::Display for ActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "window action: {self:?}")
    }
}
impl std::error::Error for ActionError {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionTicket {
    pub scope: ActionScope,
    pub id: NonZeroU64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionStatus {
    Queued,
    Issued,
    Observed,
    /// Setter returned for this original window; no OS visibility observation is claimed.
    Applied,
    Failed(ActionError),
}
impl ActionStatus {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Observed | Self::Applied | Self::Failed(_))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionRecord {
    pub ticket: ActionTicket,
    pub action: WindowAction,
    pub status: ActionStatus,
    pub observation: Option<WindowObservation>,
    pub observed_at: Option<Duration>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionAdmission {
    pub record: ActionRecord,
    pub duplicate: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionSnapshot {
    pub scope: ActionScope,
    pub revision: u64,
    pub capabilities: ActionCapabilities,
    pub observed: WindowObservation,
    pub observed_at: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowInputKind {
    PointerMotion,
    PointerButton,
    Wheel,
    Key,
    FocusGained,
    FocusLost,
    PointerEntered,
    PointerLeft,
}
/// Latest actual window event metadata; no DOM target, SDK admission or timer authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowInputActivity {
    pub scope: ActionScope,
    pub sequence: u64,
    pub kind: WindowInputKind,
    pub observed_at: Duration,
    /// Only actual focus/enter/leave events establish these; unknown is not focused/inside.
    pub focused: Option<bool>,
    pub pointer_inside: Option<bool>,
}

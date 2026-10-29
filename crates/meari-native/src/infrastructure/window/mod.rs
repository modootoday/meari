//! Winit/softbuffer surface with our AccessKit action handler, rather than the upstream TODO.
use crate::infrastructure::native_host::{self, access};
use accesskit_xplat::{Adapter, WindowEvent as AccessibilityEvent};
use anyrender_vello_cpu::VelloCpuWindowRenderer;
use blitz_shell::{BlitzApplication, BlitzShellProxy, WindowConfig};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{WindowAttributes, WindowId},
};

struct A11yEvent {
    window: WindowId,
    event: AccessibilityEvent,
}
struct Handler {
    window: WindowId,
    proxy: BlitzShellProxy,
}
struct Stop;
mod actions;
mod driver;
mod input;
pub use actions::NativeWindowActions;
mod forwarding;
mod lifecycle;
mod parent;
mod parent_client;
#[cfg(feature = "test-support")]
pub use parent::test_support;
pub use parent_client::{NativeCheckedParentClient, NativeParentClientError};
mod plain_lifecycle;
mod preparation;
mod scheduling;
mod timers;
mod user_close;
pub use timers::{NativeHostTimer, NativeHostTimerToken};
mod startup;
mod surface_slots;
pub use startup::{
    NativeAttachment, NativeLoopOwner, NativeRunError, NativeRunOutcome, NativeStartup,
    NativeStartupAttachFailure, NativeStartupContext, NativeStartupController, NativeStartupError,
    NativeStartupPoll, NativeStartupStamp, NativeStartupWake, loop_api,
};
pub use surface_slots::{
    NativeSurfaceAccessError, NativeSurfaceRegistrar, NativeSurfaceRegistrationFailure,
    NativeSurfaceReservation, NativeSurfaceSlot, NativeSurfaceSnapshot,
};
mod wake;
pub use driver::{NativeFrameSubmission, NativeWindowDriver};
pub use parent::{
    NativeChildRegistrar, NativeFrameObservation, NativeNoticeError, NativeParentLease,
    NativeScopedParent, NativeWindowGeometry, NativeWindowNotice, NativeWindowNoticeKind,
    NativeWindowObservation, NativeWindowObserver,
};
pub use preparation::{
    NativeAttachFailure, NativeAttachProblem, NativeAttachStage, NativeWindowPreparation,
};
pub use wake::NativeWindowWake;
struct Application {
    inner: BlitzApplication<VelloCpuWindowRenderer>,
    adapters: HashMap<WindowId, Adapter>,
    errors: Arc<std::sync::Mutex<Vec<String>>>,
    frames: HashMap<WindowId, super::checked_window::WindowFrames>,
    hosts: HashMap<u64, Box<dyn native_host::window_port::WindowHostPort>>,
    actions: HashMap<u64, std::rc::Rc<actions::ActionBinding>>,
    wakes: HashMap<u64, NativeWindowWake>,
    lifecycles: HashMap<u64, Box<dyn lifecycle::WindowLifecyclePort>>,
    clock: scheduling::NativeClock,
    turns: HashMap<u64, scheduling::HostSchedule>,
    closes: HashMap<u64, scheduling::HostSchedule>,
    pending_removals: HashSet<WindowId>,
    #[cfg(test)]
    timer_destroy_traces: HashMap<WindowId, Arc<std::sync::Mutex<timers::TimerBook>>>,
    lifecycle_deadline: Option<(std::time::Instant, winit::event_loop::ControlFlow)>,
    exit_when_empty: bool,
}

pub use self::event_loop::*;
use self::handler::event_loop;
mod handler;

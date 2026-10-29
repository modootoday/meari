use super::super::scheduling::NativeClock;
use super::*;
use crate::infrastructure::native_host::{HostScheduleLimits, NativeDocumentHandle};
use anyrender_vello_cpu::VelloCpuWindowRenderer;
use blitz_shell::{BlitzApplication, BlitzShellProxy};
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) struct Authority {
    pub stamp: NativeStartupStamp,
}
impl Authority {
    pub fn new() -> Result<Rc<Self>, NativeStartupError> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let owner = NEXT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| NativeStartupError::CounterExhausted)?;
        Ok(Rc::new(Self {
            stamp: NativeStartupStamp {
                owner,
                generation: 1,
            },
        }))
    }
}
/// Owns creation and execution of one pinned loop; it cannot be cloned or run twice.
/// ```compile_fail,E0382
/// use meari_native::infrastructure::window::{NativeLoopOwner,NativeStartup,NativeStartupController};
/// fn twice<S:NativeStartupController>(owner:NativeLoopOwner,a:NativeStartup,b:NativeStartup,one:S,two:S){
///     let _=owner.run_startup(a,one);
///     let _=owner.run_startup(b,two);
/// }
/// ```
pub struct NativeLoopOwner {
    pub(super) event_loop: Option<blitz_shell::EventLoop>,
    pub(super) authority: Rc<Authority>,
    pub(super) limits: HostScheduleLimits,
    issued: bool,
    pub(super) clock: NativeClock,
    pub(super) run_attempted: bool,
}
impl NativeLoopOwner {
    pub fn new(limits: HostScheduleLimits) -> Result<Self, NativeStartupError> {
        let mut builder = blitz_shell::EventLoop::builder();
        // This !Send owner keeps the loop on the creating thread, including Rust test UI threads.
        #[cfg(windows)]
        {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            builder.with_any_thread(true);
        }
        let event_loop = builder.build().map_err(NativeStartupError::Loop)?;
        Ok(Self {
            event_loop: Some(event_loop),
            authority: Authority::new()?,
            limits,
            issued: false,
            clock: NativeClock::new(),
            run_attempted: false,
        })
    }
    pub fn startup(&mut self) -> Result<NativeStartup, NativeStartupError> {
        if self.issued {
            return Err(NativeStartupError::AlreadyIssued);
        }
        let event_loop = self.event_loop.as_ref().ok_or(NativeStartupError::Closed)?;
        let (proxy, events) = BlitzShellProxy::new(event_loop.create_proxy());
        self.issued = true;
        Ok(NativeStartup {
            authority: self.authority.clone(),
            inner: Some(BlitzApplication::new(proxy.clone(), events)),
            signal: wake::Shared::new(proxy),
            clock: self.clock,
            document: None,
            handoff_attempted: false,
        })
    }
    pub fn run_startup<S: NativeStartupController>(
        self,
        startup: NativeStartup,
        controller: S,
    ) -> NativeRunOutcome<S> {
        runner::run(self, startup, controller)
    }
}
pub struct NativeStartup {
    pub(super) authority: Rc<Authority>,
    pub(super) inner: Option<BlitzApplication<VelloCpuWindowRenderer>>,
    pub(super) signal: std::sync::Arc<wake::Shared>,
    pub(super) clock: NativeClock,
    pub(super) document: Option<NativeDocumentHandle>,
    pub(super) handoff_attempted: bool,
}
impl NativeStartup {
    pub fn stamp(&self) -> NativeStartupStamp {
        self.authority.stamp
    }
    pub fn wake_handle(&self) -> NativeStartupWake {
        NativeStartupWake {
            shared: self.signal.clone(),
        }
    }
    pub fn retained_document(&self) -> Option<NativeDocumentHandle> {
        self.document.clone()
    }
}
impl Drop for NativeStartup {
    fn drop(&mut self) {
        self.signal.close();
    }
}

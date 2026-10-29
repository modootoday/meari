use super::*;
use super::{
    lifecycle::Lifecycle,
    preparation::{
        NativeAttachFailure, NativeAttachProblem, NativeAttachStage, NativeWindowPreparation,
    },
};
use crate::application::window_lifecycle::NativeChild;
use crate::infrastructure::native_host::{
    ChildWindowHost, NativeDocumentHandle, NativeHostRuntime, NativeWindowHost,
};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicBool, AtomicU8},
};

/// Attach to the caller's pinned winit loop. This does not import an eframe renderer/context.
pub struct NativeWindowDriver {
    pub(super) application: Application,
    pub(super) wake: NativeWindowWake,
}
/// Checked renderer submission, not an OS compositor presentation acknowledgement.
#[derive(Clone, Debug)]
pub struct NativeFrameSubmission {
    pub ticket: crate::domain::graphics::FrameTicket,
    pub width: u32,
    pub height: u32,
    pub commands: usize,
}
impl NativeWindowDriver {
    pub fn attach_prepared<H: NativeWindowHost, C: NativeChild>(
        preparation: NativeWindowPreparation<C>,
        runtime: Rc<RefCell<NativeHostRuntime<ChildWindowHost<H, C>>>>,
    ) -> Result<Self, Box<NativeAttachFailure<H, C>>> {
        Self::attach_prepared_owned(preparation, runtime, None)
    }
    pub(super) fn attach_prepared_owned<H: NativeWindowHost, C: NativeChild>(
        mut preparation: NativeWindowPreparation<C>,
        runtime: Rc<RefCell<NativeHostRuntime<ChildWindowHost<H, C>>>>,
        authority: Option<&Rc<super::startup::Authority>>,
    ) -> Result<Self, Box<NativeAttachFailure<H, C>>> {
        if let Some(expected) = &preparation.startup_authority
            && !authority.is_some_and(|actual| Rc::ptr_eq(expected, actual))
        {
            preparation.handoff_attempted = true;
            return Err(Box::new(NativeAttachFailure {
                stage: NativeAttachStage::Validation,
                problem: NativeAttachProblem::NotPristine,
                preparation,
                runtime,
            }));
        }
        let validation = if preparation.handoff_attempted {
            Err(NativeAttachProblem::NotPristine)
        } else {
            preparation.validate(&runtime)
        };
        preparation.handoff_attempted = true;
        if let Err(problem) = validation {
            return Err(Box::new(NativeAttachFailure {
                stage: NativeAttachStage::Validation,
                problem,
                preparation,
                runtime,
            }));
        }
        if let Err(error) = preparation.binding.seed.bind_once(
            preparation.document.id(),
            preparation.wake.lifecycle_waker(),
        ) {
            return Err(Box::new(NativeAttachFailure {
                stage: NativeAttachStage::SinkBinding,
                problem: NativeAttachProblem::Lifecycle(error),
                preparation,
                runtime,
            }));
        }
        // No fallible assembly after port() marks the existing runtime attached.
        let port = match native_host::window_port::port(runtime.clone()) {
            Ok(port) => port,
            Err(error) => {
                return Err(Box::new(NativeAttachFailure {
                    stage: NativeAttachStage::RuntimeHandoff,
                    problem: NativeAttachProblem::Host(error),
                    preparation,
                    runtime,
                }));
            }
        };
        let host = preparation.document.id();
        let lifecycle: Box<dyn super::lifecycle::WindowLifecyclePort> = Box::new(Lifecycle::new(
            preparation.binding.clone(),
            preparation.children.clone(),
            preparation.clock,
        ));
        let wake = preparation.wake.clone();
        let inner = preparation
            .inner
            .take()
            .expect("validated single-use preparation");
        Ok(Self {
            application: Application {
                inner,
                adapters: HashMap::new(),
                errors: Arc::new(std::sync::Mutex::new(Vec::new())),
                frames: HashMap::new(),
                hosts: HashMap::from([(host, port)]),
                actions: HashMap::from([(host, preparation.actions.clone())]),
                wakes: HashMap::from([(host, wake.clone())]),
                lifecycles: HashMap::from([(host, lifecycle)]),
                clock: scheduling::NativeClock::new(),
                turns: HashMap::new(),
                closes: HashMap::new(),
                pending_removals: HashSet::new(),
                #[cfg(test)]
                timer_destroy_traces: HashMap::new(),
                lifecycle_deadline: None,
                exit_when_empty: false,
            },
            wake,
        })
    }
    pub(super) fn from_shell(
        inner: BlitzApplication<VelloCpuWindowRenderer>,
        wake: NativeWindowWake,
        port: Box<dyn native_host::window_port::WindowHostPort>,
    ) -> Self {
        let host = wake.host;
        Self {
            application: Application {
                inner,
                adapters: HashMap::new(),
                errors: Arc::new(std::sync::Mutex::new(Vec::new())),
                frames: HashMap::new(),
                hosts: HashMap::from([(host, port)]),
                actions: HashMap::new(),
                wakes: HashMap::from([(host, wake.clone())]),
                lifecycles: HashMap::new(),
                clock: scheduling::NativeClock::new(),
                turns: HashMap::new(),
                closes: HashMap::new(),
                pending_removals: HashSet::new(),
                #[cfg(test)]
                timer_destroy_traces: HashMap::new(),
                lifecycle_deadline: None,
                exit_when_empty: false,
            },
            wake,
        }
    }
    pub fn attach<H: NativeWindowHost>(
        event_loop: &blitz_shell::EventLoop,
        runtime: Rc<RefCell<NativeHostRuntime<H>>>,
        options: crate::application::WindowOptions,
    ) -> Result<Self, String> {
        Self::build(event_loop, runtime, options, false)
    }
    pub(super) fn build<H: NativeWindowHost>(
        event_loop: &blitz_shell::EventLoop,
        runtime: Rc<RefCell<NativeHostRuntime<H>>>,
        options: crate::application::WindowOptions,
        exit_when_empty: bool,
    ) -> Result<Self, String> {
        let handle: NativeDocumentHandle = runtime
            .try_borrow()
            .map_err(|_| "native host is borrowed".to_owned())?
            .document();
        let (inner, wake) = shell(event_loop, &handle, options)?;
        let port = native_host::window_port::port(runtime)?;
        Ok(Self {
            application: Application {
                inner,
                adapters: HashMap::new(),
                errors: Arc::new(std::sync::Mutex::new(Vec::new())),
                frames: HashMap::new(),
                hosts: HashMap::from([(handle.id(), port)]),
                actions: HashMap::new(),
                wakes: HashMap::from([(handle.id(), wake.clone())]),
                lifecycles: HashMap::new(),
                clock: scheduling::NativeClock::new(),
                turns: HashMap::new(),
                closes: HashMap::new(),
                pending_removals: HashSet::new(),
                #[cfg(test)]
                timer_destroy_traces: HashMap::new(),
                lifecycle_deadline: None,
                exit_when_empty,
            },
            wake,
        })
    }
    pub fn wake_handle(&self) -> NativeWindowWake {
        self.wake.clone()
    }
    /// Legacy plain attachments have no prepared coordinator and return None.
    pub fn window_actions(&self) -> Option<super::NativeWindowActions> {
        self.application
            .actions
            .get(&self.wake.host)
            .map(|binding| binding.handle())
    }
    pub fn last_submission(&self) -> Option<NativeFrameSubmission> {
        self.application
            .frames
            .values()
            .find_map(|frames| frames.submission())
    }
    pub fn presentation_errors(&self) -> Vec<String> {
        self.application.errors.lock().map_or_else(
            |_| vec!["native diagnostics lock poisoned".into()],
            |errors| errors.clone(),
        )
    }
}
impl ApplicationHandler for NativeWindowDriver {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.application.can_create_surfaces(event_loop);
    }
    fn destroy_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.application.destroy_surfaces(event_loop);
    }
    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.application.window_event(event_loop, id, event);
    }
    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.application.proxy_wake_up(event_loop);
    }
    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.application.about_to_wait(event_loop);
    }
}
impl Drop for NativeWindowDriver {
    fn drop(&mut self) {
        self.application.restore_window_cursors();
        self.application.frames.clear();
        for lifecycle in self.application.lifecycles.values() {
            lifecycle.abandon();
        }
        for wake in self.application.wakes.values() {
            wake.close();
        }
        self.application.inner.windows.clear();
        self.application.adapters.clear();
        for port in self.application.hosts.values() {
            let _ = port.removed(true);
        }
    }
}

pub(super) fn shell(
    event_loop: &blitz_shell::EventLoop,
    handle: &NativeDocumentHandle,
    options: crate::application::WindowOptions,
) -> Result<(BlitzApplication<VelloCpuWindowRenderer>, NativeWindowWake), String> {
    let (proxy, events) = BlitzShellProxy::new(event_loop.create_proxy());
    let mut inner = BlitzApplication::new(proxy, events);
    let wake = configure_shell(&mut inner, handle, options)?;
    Ok((inner, wake))
}
pub(super) fn configure_shell(
    inner: &mut BlitzApplication<VelloCpuWindowRenderer>,
    handle: &NativeDocumentHandle,
    options: crate::application::WindowOptions,
) -> Result<NativeWindowWake, String> {
    if !inner.pending_windows.is_empty() || !inner.windows.is_empty() {
        return Err("native shell already owns a document".into());
    }
    options.validate()?;
    let viewport = handle
        .try_with(|document| document.viewport.clone())
        .map_err(|error| error.to_string())?;
    let proxy = inner.proxy.clone();
    inner.add_window(WindowConfig::with_attributes(
        access::facade(handle.clone()),
        VelloCpuWindowRenderer::new(),
        WindowAttributes::default()
            .with_title(options.title)
            .with_surface_size(winit::dpi::LogicalSize::new(
                f64::from(viewport.width),
                f64::from(viewport.height),
            ))
            .with_visible(false),
    ));
    let wake = NativeWindowWake {
        host: handle.id(),
        proxy,
        pending: Arc::new(AtomicU8::new(0)),
        closed: Arc::new(AtomicBool::new(false)),
        timers: Default::default(),
        user_close: Default::default(),
    };
    Ok(wake)
}

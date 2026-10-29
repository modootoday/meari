use super::super::{
    NativeWindowDriver,
    scheduling::{HostSchedule, NativeClock},
};
use super::*;
use crate::{
    application::window_lifecycle::MonotonicClock,
    infrastructure::native_host::{HostLifecycle, HostScheduleLimits},
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    time::Instant,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow},
    window::WindowId,
};

struct State<S: NativeStartupController> {
    controller: Option<S>,
    startup: Option<NativeStartup>,
    driver: Option<NativeWindowDriver>,
    runtime: Option<Rc<RefCell<NativeHostRuntime<S::Host>>>>,
    recovery: Option<NativeAttachment<S::Host>>,
    failure: Option<NativeRunError<S::Error>>,
    hook_pending: bool,
}
struct Runner<S: NativeStartupController> {
    state: Rc<RefCell<State<S>>>,
    authority: Rc<Authority>,
    clock: NativeClock,
    limits: HostScheduleLimits,
    observation: HostSchedule,
    has_surfaces: bool,
    started: bool,
    owned_flow: Option<(Instant, ControlFlow)>,
}

pub(super) fn run<S: NativeStartupController>(
    mut owner: NativeLoopOwner,
    startup: NativeStartup,
    controller: S,
) -> NativeRunOutcome<S> {
    if std::mem::replace(&mut owner.run_attempted, true)
        || !Rc::ptr_eq(&owner.authority, &startup.authority)
        || startup.inner.is_none()
    {
        startup.signal.close();
        return NativeRunOutcome {
            controller,
            startup: Some(startup),
            runtime: None,
            recovery: None,
            loop_owner: Some(owner),
            result: Err(NativeRunError::Startup(NativeStartupError::Stale)),
        };
    }
    let event_loop = owner.event_loop.take().expect("fresh native loop owner");
    let state = Rc::new(RefCell::new(State {
        controller: Some(controller),
        startup: Some(startup),
        driver: None,
        runtime: None,
        recovery: None,
        failure: None,
        hook_pending: false,
    }));
    let app = Runner {
        state: state.clone(),
        authority: owner.authority.clone(),
        clock: owner.clock,
        limits: owner.limits,
        observation: HostSchedule::default(),
        has_surfaces: false,
        started: false,
        owned_flow: None,
    };
    let loop_result = event_loop.run_app(app);
    let mut state = state.borrow_mut();
    let result = state.failure.take().map_or_else(
        || loop_result.map_err(|error| NativeRunError::Startup(NativeStartupError::Loop(error))),
        Err,
    );
    NativeRunOutcome {
        controller: state
            .controller
            .take()
            .expect("controller returned after loop exit"),
        startup: state.startup.take(),
        runtime: state.runtime.take(),
        recovery: state.recovery.take(),
        loop_owner: None,
        result,
    }
}
impl<S: NativeStartupController> Runner<S> {
    fn fail(&mut self, event_loop: &dyn ActiveEventLoop, error: NativeRunError<S::Error>) {
        self.state.borrow_mut().failure = Some(error);
        event_loop.exit();
    }
    fn drive(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        call: impl FnOnce(&mut NativeWindowDriver),
    ) {
        let driver = self.state.borrow_mut().driver.take();
        if let Some(mut driver) = driver {
            let result = catch_unwind(AssertUnwindSafe(|| call(&mut driver)));
            let empty = driver.application.hosts.is_empty()
                && driver.application.pending_removals.is_empty();
            self.state.borrow_mut().driver = Some(driver);
            if result.is_err() {
                self.fail(event_loop, NativeRunError::Panicked);
                return;
            }
            if empty {
                let forced = self.state.borrow().runtime.as_ref().is_some_and(|runtime| {
                    runtime.try_borrow().is_ok_and(|runtime| {
                        runtime.document().lifecycle() == HostLifecycle::ForcedIncomplete
                    })
                });
                if forced {
                    self.fail(event_loop, NativeRunError::ForcedIncomplete);
                } else {
                    event_loop.exit();
                }
            }
        }
    }
    fn attached(&mut self, event_loop: &dyn ActiveEventLoop) {
        let pending = self.state.borrow().hook_pending;
        if !pending {
            return;
        }
        let (mut controller, runtime, wake) = {
            let mut state = self.state.borrow_mut();
            state.hook_pending = false;
            (
                state.controller.take().expect("startup controller"),
                state.runtime.as_ref().expect("attached runtime").clone(),
                state
                    .driver
                    .as_ref()
                    .expect("attached driver")
                    .wake_handle(),
            )
        };
        let result = catch_unwind(AssertUnwindSafe(|| controller.attached(runtime, wake)));
        self.state.borrow_mut().controller = Some(controller);
        match result {
            Ok(Ok(())) => (),
            Ok(Err(error)) => self.fail(event_loop, NativeRunError::Consumer(error)),
            Err(_) => self.fail(event_loop, NativeRunError::Panicked),
        }
    }
    fn observe(&mut self, event_loop: &dyn ActiveEventLoop, force: bool) {
        if self.state.borrow().driver.is_some() || self.state.borrow().failure.is_some() {
            return;
        }
        let now = self.clock.now();
        let notified = self
            .state
            .borrow()
            .startup
            .as_ref()
            .expect("startup owner")
            .signal
            .take();
        let notified = match notified {
            Ok(value) => value,
            Err(error) => {
                self.fail(event_loop, NativeRunError::Startup(error));
                return;
            }
        };
        if !force && !notified && !self.observation.due(now) {
            return;
        }
        if !self.observation.ready_at(now, force) || !self.observation.begin(force) {
            return;
        }
        let (mut controller, mut startup) = {
            let mut state = self.state.borrow_mut();
            (
                state.controller.take().expect("controller"),
                state.startup.take().expect("startup"),
            )
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            controller.observe(&mut NativeStartupContext {
                startup: &mut startup,
            })
        }));
        {
            let mut state = self.state.borrow_mut();
            state.controller = Some(controller);
            state.startup = Some(startup);
        }
        match result {
            Err(_) => self.fail(event_loop, NativeRunError::Panicked),
            Ok(Err(error)) => self.fail(event_loop, NativeRunError::Consumer(error)),
            Ok(Ok(NativeStartupPoll::Stop)) => {
                self.observation.complete();
                event_loop.exit();
            }
            Ok(Ok(NativeStartupPoll::Pending(retry))) => {
                if let Err(error) =
                    self.observation
                        .defer_after(&self.clock, retry, Some(self.limits))
                {
                    self.fail(event_loop, NativeRunError::Scheduling(error));
                }
            }
            Ok(Ok(NativeStartupPoll::Attach(mut token))) => {
                self.state.borrow_mut().runtime = Some(token.runtime.clone());
                match token.run(&self.authority, self.limits) {
                    Err(error) => {
                        self.state.borrow_mut().recovery = Some(token);
                        self.fail(event_loop, NativeRunError::Startup(error));
                    }
                    Ok(mut driver) => {
                        driver.application.clock = self.clock;
                        let signal = self
                            .state
                            .borrow()
                            .startup
                            .as_ref()
                            .expect("startup")
                            .signal
                            .clone();
                        if let Err(error) = signal.bind(driver.wake_handle()) {
                            self.state.borrow_mut().driver = Some(driver);
                            self.fail(event_loop, NativeRunError::Startup(error));
                            return;
                        }
                        self.observation.complete();
                        {
                            let mut state = self.state.borrow_mut();
                            state.driver = Some(driver);
                            state.hook_pending = true;
                        }
                        self.attached(event_loop);
                        if self.has_surfaces && self.state.borrow().failure.is_none() {
                            self.drive(event_loop, |driver| driver.can_create_surfaces(event_loop));
                        }
                    }
                }
            }
        }
    }
    fn startup_deadline(&mut self, event_loop: &dyn ActiveEventLoop) {
        let next = match self.observation.absolute_deadline(self.clock.origin) {
            Ok(next) => next,
            Err(error) => {
                self.fail(event_loop, NativeRunError::Scheduling(error));
                None
            }
        };
        super::super::scheduling::apply_deadline(event_loop, next, &mut self.owned_flow);
    }
}
impl<S: NativeStartupController> ApplicationHandler for Runner<S> {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.has_surfaces = true;
        if self.state.borrow().driver.is_some() {
            self.drive(event_loop, |driver| driver.can_create_surfaces(event_loop));
            self.attached(event_loop);
        } else {
            let first = !std::mem::replace(&mut self.started, true);
            self.observe(event_loop, first);
        }
    }
    fn destroy_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.has_surfaces = false;
        self.drive(event_loop, |driver| driver.destroy_surfaces(event_loop));
    }
    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.drive(event_loop, |driver| {
            driver.window_event(event_loop, id, event)
        });
    }
    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.state.borrow().driver.is_some() {
            self.drive(event_loop, |driver| driver.proxy_wake_up(event_loop));
        } else {
            self.observe(event_loop, false);
        }
    }
    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.state.borrow().driver.is_some() {
            self.observation.complete();
            self.startup_deadline(event_loop);
            self.drive(event_loop, |driver| driver.about_to_wait(event_loop));
        } else {
            self.observe(event_loop, false);
            self.startup_deadline(event_loop);
        }
    }
}
impl<S: NativeStartupController> Drop for Runner<S> {
    fn drop(&mut self) {
        let driver = {
            let mut state = self.state.borrow_mut();
            if let Some(startup) = &state.startup {
                startup.signal.close();
            }
            state.driver.take()
        };
        drop(driver);
    }
}

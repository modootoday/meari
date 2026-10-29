#![cfg(all(windows, feature = "window"))]
//! Original prepared driver dispatch. Ignored until an explicit isolated OS allocation.
use meari_native::{
    application::{WindowOptions, window_actions::*, window_lifecycle::*},
    domain::Viewport,
    infrastructure::{document::InteractiveDocument, native_host::*, window::*},
};
use std::{
    cell::RefCell,
    num::NonZeroU64,
    rc::Rc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    platform::windows::EventLoopBuilderExtWindows,
    window::WindowId,
};
struct NoChild;
impl NativeChild for NoChild {
    type Error = std::io::Error;
    fn parent_lost(&mut self) {
        panic!("fixture registers no child");
    }
    fn begin_cleanup(
        &mut self,
        _: CleanupRequest,
        _: CompletionSignal,
    ) -> Result<CleanupProgress, Self::Error> {
        panic!("fixture registers no child");
    }
    fn observe_cleanup(&mut self, _: CleanupRequest) -> Result<CleanupProgress, Self::Error> {
        panic!("fixture registers no child");
    }
}
#[derive(Default)]
struct Consumer {
    allow: bool,
    causes: Vec<CloseCause>,
    completions: usize,
    closes: usize,
    observed: usize,
}
impl NativeWindowHost for Consumer {
    type Error = std::io::Error;
    type CloseCheckpoint = u8;
    fn admit_callback(
        &mut self,
        _: &HostStamp,
        _: &meari::application::host_callbacks::PendingCallback,
    ) -> IntentAdmission<Self::Error> {
        panic!("fixture has no callback");
    }
    fn after_dispatch(&mut self, turn: HostTurn) -> Result<HostEffects, Self::Error> {
        if turn.reason == HostTurnReason::Completion {
            self.completions += 1;
        }
        Ok(Default::default())
    }
    fn can_close(&mut self, _: &HostStamp, cause: CloseCause) -> Result<bool, Self::Error> {
        self.causes.push(cause);
        Ok(cause != CloseCause::User || self.allow)
    }
    fn close(&mut self, request: CloseRequest<u8>) -> CloseDecision<u8, Self::Error> {
        if request.cause != CloseCause::User {
            return CloseDecision::HostCleanupComplete { checkpoint: 7 };
        }
        self.closes += 1;
        CloseDecision::Pending { checkpoint: 7 }
    }
    fn observe_close(&mut self, request: CloseRequest<u8>) -> CloseDecision<u8, Self::Error> {
        if request.cause != CloseCause::User {
            return CloseDecision::HostCleanupComplete { checkpoint: 7 };
        }
        assert_eq!(request.checkpoint, Some(7));
        self.observed += 1;
        CloseDecision::HostCleanupComplete { checkpoint: 7 }
    }
    fn next_close_observation(&self, _: &u8) -> HostRetry {
        HostRetry::After(Duration::from_millis(5))
    }
}
type Runtime = NativeHostRuntime<ChildWindowHost<Consumer, NoChild>>;
struct Probe {
    driver: NativeWindowDriver,
    runtime: Rc<RefCell<Runtime>>,
    handle: NativeDocumentHandle,
    timer: Option<NativeHostTimer>,
    token: Option<NativeHostTimerToken>,
    phase: u8,
    baseline: usize,
    timer_baseline: usize,
    deadline: Instant,
    result: Rc<RefCell<Option<Result<(), String>>>>,
}
impl Probe {
    fn advance(&mut self) -> Result<bool, String> {
        if Instant::now() >= self.deadline {
            return Err(format!(
                "user close OS dispatch deadline phase {}",
                self.phase
            ));
        }
        match self.phase {
            0 if self.driver.last_submission().is_some() => {
                let actions = self
                    .driver
                    .window_actions()
                    .ok_or("original actions missing")?;
                let scope = match actions.snapshot() {
                    Ok(snapshot) => snapshot.scope,
                    Err(ActionError::NotReady) => return Ok(false),
                    Err(error) => return Err(error.to_string()),
                };
                self.baseline = self.runtime.borrow().host().host().completions;
                let wake = self.driver.wake_handle();
                let timer = wake
                    .timer(&self.handle)
                    .map_err(|error| error.to_string())?;
                self.token = Some(
                    timer
                        .schedule_after(Duration::from_secs(60))
                        .map_err(|error| error.to_string())?,
                );
                actions
                    .request(
                        scope,
                        NonZeroU64::new(1).unwrap(),
                        WindowAction::SetCursorVisible(true),
                    )
                    .map_err(|error| error.to_string())?;
                wake.wake().map_err(|error| error.to_string())?;
                wake.request_user_close(scope)
                    .map_err(|error| error.to_string())?;
                self.timer = Some(timer);
                self.phase = 1;
            }
            1 if self.runtime.borrow().host().host().causes == [CloseCause::User] => {
                let actions = self
                    .driver
                    .window_actions()
                    .ok_or("actions lost after veto")?;
                if actions
                    .record(NonZeroU64::new(1).unwrap())
                    .map_err(|error| error.to_string())?
                    .is_none_or(|record| record.status != ActionStatus::Applied)
                {
                    return Ok(false);
                }
                assert_eq!(self.handle.lifecycle(), HostLifecycle::Open);
                let completions = self.runtime.borrow().host().host().completions;
                // The original action adapter can post one additional acknowledgement wake.
                assert!(
                    (self.baseline + 1..=self.baseline + 2).contains(&completions),
                    "missing coalesced completion or duplicate acknowledgements"
                );
                self.timer_baseline = completions;
                assert_eq!(self.runtime.borrow().host().host().closes, 0);
                let timer = self.timer.as_ref().unwrap();
                assert!(
                    timer
                        .cancel(self.token.as_ref().unwrap())
                        .map_err(|error| error.to_string())?,
                    "User veto must preserve the original armed timer owner"
                );
                self.token = Some(
                    timer
                        .schedule_after(Duration::from_millis(20))
                        .map_err(|error| error.to_string())?,
                );
                self.phase = 2;
            }
            2 if self.runtime.borrow().host().host().completions > self.timer_baseline => {
                assert_eq!(
                    self.runtime.borrow().host().host().completions,
                    self.timer_baseline + 1,
                    "one-shot timer must not duplicate completion"
                );
                self.runtime.borrow_mut().host_mut().host_mut().allow = true;
                let scope = self
                    .driver
                    .window_actions()
                    .ok_or("actions missing before allowed close")?
                    .snapshot()
                    .map_err(|error| error.to_string())?
                    .scope;
                self.driver
                    .wake_handle()
                    .request_user_close(scope)
                    .map_err(|error| error.to_string())?;
                self.phase = 3;
            }
            3 if self.handle.lifecycle() == HostLifecycle::Closed => {
                let runtime = self.runtime.borrow();
                let host = runtime.host().host();
                assert_eq!(host.causes, vec![CloseCause::User, CloseCause::User]);
                assert_eq!(host.closes, 1);
                assert_eq!(host.observed, 1);
                assert_eq!(host.completions, self.timer_baseline + 1);
                assert!(
                    self.driver.presentation_errors().is_empty(),
                    "{:?}",
                    self.driver.presentation_errors()
                );
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }
    fn tick(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.result.borrow().is_some() {
            return;
        }
        match self.advance() {
            Ok(false) => {}
            Ok(true) => {
                *self.result.borrow_mut() = Some(Ok(()));
                event_loop.exit();
            }
            Err(error) => {
                *self.result.borrow_mut() = Some(Err(error));
                event_loop.exit();
            }
        }
    }
}
impl ApplicationHandler for Probe {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.driver.can_create_surfaces(event_loop);
        self.tick(event_loop);
    }
    fn destroy_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.driver.destroy_surfaces(event_loop);
    }
    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.driver.window_event(event_loop, id, event);
        self.tick(event_loop);
    }
    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.driver.proxy_wake_up(event_loop);
        self.tick(event_loop);
    }
    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.driver.about_to_wait(event_loop);
        self.tick(event_loop);
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            (Instant::now() + Duration::from_millis(5)).min(self.deadline),
        ));
    }
}
#[test]
#[ignore = "requires centrally allocated isolated Windows original dispatch/renderer test"]
fn actual_window_user_veto_preserves_coalesced_completion_action_and_timer_owner() {
    let source = meari::parse(
        "type Props={};export function App(props:Props){return <main>user close dispatch</main>}",
        meari::application::Language::Tsx,
    )
    .unwrap();
    let handle = NativeDocumentHandle::new(
        InteractiveDocument::new(
            source,
            "",
            Viewport {
                width: 320,
                height: 240,
                dark: false,
            },
        )
        .unwrap(),
        1,
    )
    .unwrap();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let preparation = NativeWindowPreparation::<NoChild>::new(
        &event_loop,
        handle.clone(),
        LifecycleLimits::new(1, Duration::from_secs(2)).unwrap(),
        WindowOptions {
            title: "Meari User close coalesced dispatch".into(),
        },
        None,
    )
    .unwrap();
    let runtime = Rc::new(RefCell::new(NativeHostRuntime::with_schedule_limits(
        handle.clone(),
        ChildWindowHost::new(Consumer::default(), preparation.children()),
        // Consumer explicitly asks for one 5ms close observation. Keep the
        // runtime's no-implicit-timed-retry guard and grant only that budget.
        HostScheduleLimits::new(Duration::from_millis(5), Duration::from_millis(5), 1).unwrap(),
    )));
    let driver = preparation.attach(runtime.clone()).unwrap();
    let result = Rc::new(RefCell::new(None));
    event_loop
        .run_app(Probe {
            driver,
            runtime,
            handle,
            timer: None,
            token: None,
            phase: 0,
            baseline: 0,
            timer_baseline: 0,
            deadline: Instant::now() + Duration::from_secs(10),
            result: result.clone(),
        })
        .unwrap();
    result
        .borrow_mut()
        .take()
        .expect("OS dispatch probe missing result")
        .unwrap();
}

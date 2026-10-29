#![cfg(feature = "window")]
//! External-crate typechecking: no direct winit dependency/import and no OS initialization.
use meari_native::{
    application::WindowOptions,
    infrastructure::{native_host::*, window::*},
};
use std::{cell::RefCell, rc::Rc, time::Duration};

pub fn execute<S: NativeStartupController>(
    owner: NativeLoopOwner,
    startup: NativeStartup,
    controller: S,
) -> NativeRunOutcome<S> {
    owner.run_startup(startup, controller)
}
pub fn attach<H: NativeWindowHost>(
    context: &mut NativeStartupContext<'_>,
    runtime: Rc<RefCell<NativeHostRuntime<H>>>,
    options: WindowOptions,
) -> Result<NativeAttachment<H>, Box<NativeStartupAttachFailure<H>>> {
    context.attach(runtime, options)
}
pub fn pinned_handler<A: loop_api::ApplicationHandler>(
    event: &dyn loop_api::ActiveEventLoop,
    app: &mut A,
) {
    app.about_to_wait(event);
}
pub fn observe<H: NativeWindowHost>(
    context: &mut NativeStartupContext<'_>,
    runtime: Rc<RefCell<NativeHostRuntime<H>>>,
    options: WindowOptions,
    observer: NativeWindowObserver,
) -> Result<NativeAttachment<H>, Box<NativeStartupAttachFailure<H>>> {
    context.attach_observed(runtime, options, observer)
}
pub fn observations<H: NativeWindowHost>(
    attachment: &NativeAttachment<H>,
) -> Option<NativeWindowObservation> {
    attachment.observation()
}
#[test]
fn published_facade_and_explicit_limits_typecheck_without_a_new_loop_version() {
    let limits =
        HostScheduleLimits::new(Duration::from_millis(2), Duration::from_secs(1), 5).unwrap();
    assert_eq!(limits.max_observations(), 5);
    assert!(std::any::type_name::<loop_api::EventLoop>().contains("winit"));
    assert_eq!(HostRetry::OnWake, HostRetry::OnWake);
}

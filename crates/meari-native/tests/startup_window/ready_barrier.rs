//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use std::{
    cell::RefCell,
    fmt,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{HWND, RECT},
    UI::WindowsAndMessaging::{
        FindWindowW, GetClientRect, GetWindowRect, GetWindowThreadProcessId, IsWindow, MoveWindow,
    },
};

impl NativeStartupController for HookFailure {
    type Host = Host;
    type Error = Failure;
    fn observe(
        &mut self,
        context: &mut NativeStartupContext<'_>,
    ) -> Result<NativeStartupPoll<Host>, Failure> {
        let runtime = Rc::new(RefCell::new(NativeHostRuntime::new(
            document(),
            Host {
                control: self.control.clone(),
                wake: self.wake.clone(),
            },
        )));
        self.runtime = Some(runtime.clone());
        Ok(NativeStartupPoll::Attach(
            context
                .attach(
                    runtime,
                    WindowOptions {
                        title: "Meari attached failure technical window".into(),
                    },
                )
                .map_err(|_| Failure("attach"))?,
        ))
    }
    fn attached(
        &mut self,
        runtime: Rc<RefCell<NativeHostRuntime<Host>>>,
        _: NativeWindowWake,
    ) -> Result<(), Failure> {
        assert!(Rc::ptr_eq(&runtime, self.runtime.as_ref().unwrap()));
        assert!(runtime.try_borrow_mut().is_ok());
        assert!(
            !failure_view_exists(),
            "actual fixture HWND exists before attached error"
        );
        Err(Failure("exact canonical attached failure"))
    }
}

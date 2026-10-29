#![cfg(all(windows, feature = "window"))]
//! Each ignored test must run in a separate process: winit creates one loop per process.
use meari_native::{
    application::{WindowOptions, window_lifecycle::*},
    domain::Viewport,
    infrastructure::{document::InteractiveDocument, native_host::*, window::*},
};
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

#[derive(Debug, PartialEq, Eq)]
struct Failure(&'static str);
struct ObservedHost;
struct ObservedController {
    document: NativeDocumentHandle,
    wake: Rc<RefCell<Option<NativeWindowWake>>>,
    frames: Rc<RefCell<Vec<NativeFrameObservation>>>,
    ready_commit: Rc<RefCell<Option<HostCommitReceipt>>>,
    observation: Option<NativeWindowObservation>,
    watchdog: Option<(mpsc::Sender<()>, JoinHandle<bool>)>,
}
#[derive(Default)]
struct Control {
    parent: Option<HWND>,
    resized: bool,
    requested: bool,
    child_cleanup: usize,
    close_times: Vec<Instant>,
    fail_after_view: bool,
    after_calls: usize,
    failure_calls: usize,
    command: Option<mpsc::Sender<WakeCommand>>,
}
enum WakeCommand {
    Close,
    Stop,
}
struct Host {
    control: Rc<RefCell<Control>>,
    wake: Rc<RefCell<Option<NativeWindowWake>>>,
}
struct Timer {
    worker: Option<JoinHandle<()>>,
    release: Option<mpsc::Sender<()>>,
    calls: usize,
    joined: usize,
    first: Option<Instant>,
    deadline: Instant,
}

struct ParentProbe {
    lease: NativeParentLease,
    control: Rc<RefCell<Control>>,
}
struct WindowController {
    control: Rc<RefCell<Control>>,
    wake: Rc<RefCell<Option<NativeWindowWake>>>,
    mounted: bool,
    hook_calls: usize,
    watchdog: Option<(mpsc::Sender<WakeCommand>, JoinHandle<bool>)>,
}
impl WindowController {
    fn stop_watchdog(&mut self) -> bool {
        if let Some((sender, worker)) = self.watchdog.take() {
            let _ = sender.send(WakeCommand::Stop);
            worker.join().unwrap()
        } else {
            false
        }
    }
}
struct HookFailure {
    runtime: Option<Rc<RefCell<NativeHostRuntime<Host>>>>,
    control: Rc<RefCell<Control>>,
    wake: Rc<RefCell<Option<NativeWindowWake>>>,
}

mod harness;
use self::harness::*;
mod ready_barrier;
use self::ready_barrier::*;
mod frame_stamps;

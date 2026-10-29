use meari_native::application::window_lifecycle::*;
use std::{
    cell::{Cell, RefCell},
    io,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Wake, Waker},
    time::Duration,
};

#[path = "window_lifecycle/close.rs"]
mod close;
#[path = "window_lifecycle/completion.rs"]
mod completion;
#[path = "window_lifecycle/state.rs"]
mod state;

#[derive(Clone, Default)]
struct Clock(Rc<Cell<Duration>>);
impl MonotonicClock for Clock {
    fn now(&self) -> Duration {
        self.0.get()
    }
}
struct WakeCount(AtomicUsize);
impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
#[derive(Clone, Copy)]
enum Mode {
    Complete,
    Pending,
    FailOnce,
    Early,
}
struct Control {
    mode: Mode,
    starts: usize,
    observations: usize,
    losses: usize,
    done: bool,
    signals: Vec<CompletionSignal>,
    hook: Option<Box<dyn FnMut()>>,
}
fn control(mode: Mode) -> Rc<RefCell<Control>> {
    Rc::new(RefCell::new(Control {
        mode,
        starts: 0,
        observations: 0,
        losses: 0,
        done: false,
        signals: Vec::new(),
        hook: None,
    }))
}
struct Child(Rc<RefCell<Control>>);
impl NativeChild for Child {
    type Error = io::Error;
    fn parent_lost(&mut self) {
        self.0.borrow_mut().losses += 1;
    }
    fn begin_cleanup(
        &mut self,
        _: CleanupRequest,
        signal: CompletionSignal,
    ) -> Result<CleanupProgress, io::Error> {
        let (mode, starts, hook) = {
            let mut control = self.0.borrow_mut();
            control.starts += 1;
            control.signals.push(signal.clone());
            (control.mode, control.starts, control.hook.take())
        };
        if let Some(mut hook) = hook {
            hook();
            self.0.borrow_mut().hook = Some(hook);
        }
        match mode {
            Mode::Complete => Ok(CleanupProgress::Complete),
            Mode::Pending => Ok(CleanupProgress::Pending),
            Mode::FailOnce if starts == 1 => Err(io::Error::other("technical teardown failure")),
            Mode::FailOnce => Ok(CleanupProgress::Pending),
            Mode::Early => {
                self.0.borrow_mut().done = true;
                signal.notify();
                Ok(CleanupProgress::Pending)
            }
        }
    }
    fn observe_cleanup(&mut self, _: CleanupRequest) -> Result<CleanupProgress, io::Error> {
        let mut control = self.0.borrow_mut();
        control.observations += 1;
        Ok(if control.done {
            CleanupProgress::Complete
        } else {
            CleanupProgress::Pending
        })
    }
}
type Owner = Rc<WindowLifecycle<Child>>;
fn owner(capacity: usize) -> (Owner, Clock, Arc<WakeCount>) {
    let clock = Clock::default();
    let wakes = Arc::new(WakeCount(AtomicUsize::new(0)));
    let owner = Rc::new(
        WindowLifecycle::new(
            LifecycleLimits::new(capacity, Duration::from_secs(5)).unwrap(),
            clock.clone(),
            Waker::from(wakes.clone()),
        )
        .unwrap(),
    );
    (owner, clock, wakes)
}
fn ready(owner: &Owner) -> WindowStamp {
    owner.ready(ReadyKind::NewWindow).unwrap()
}
fn register(owner: &Owner, state: &Rc<RefCell<Control>>) -> ChildLease {
    owner
        .register(owner.stamp().unwrap(), |_| Ok(Child(state.clone())))
        .unwrap()
}
fn finish(state: &Rc<RefCell<Control>>) {
    let signal = {
        let mut state = state.borrow_mut();
        state.done = true;
        state.signals.last().unwrap().clone()
    };
    signal.notify();
}

use super::*;
use crate::{application::window_lifecycle::*, infrastructure::native_host::*};
use std::{cell::RefCell, rc::Rc};
#[derive(Debug)]
struct Failure(u64);
#[derive(Default)]
struct Control {
    after_error: Option<u64>,
    wait: bool,
    fail_close: bool,
    unavailable_close: bool,
    closes: usize,
    close_observations: usize,
    hints: usize,
}
struct Host(Rc<RefCell<Control>>);
#[derive(Clone)]
struct Clock;
#[derive(Default)]
struct ChildState {
    done: bool,
    signal: Option<CompletionSignal>,
}
struct Child(Rc<RefCell<ChildState>>);

#[derive(Default)]
struct ObservationState {
    done: bool,
    fail_once: bool,
    pending: bool,
    starts: usize,
    observations: usize,
    dropped: usize,
    signal: Option<CompletionSignal>,
}
struct ObservationChild {
    state: Rc<RefCell<ObservationState>>,
    diagnostic: Failure,
}

mod harness;
use self::harness::*;
#[cfg(test)]
mod consumer_errors;
#[cfg(test)]
#[cfg(feature = "window")]
mod retry_wakes;

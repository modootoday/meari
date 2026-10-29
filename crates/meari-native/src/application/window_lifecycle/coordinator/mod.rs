use super::{
    authority::{Authority, ChildLease},
    ports::*,
    state::State,
};
use crate::domain::window_lifecycle::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    task::Waker,
};

#[derive(Debug)]
pub enum RegistrationProblem<E> {
    Lifecycle(LifecycleError),
    Creation(E),
}
#[derive(Debug)]
pub struct RegistrationFailure<E> {
    pub problem: RegistrationProblem<E>,
    /// A partial child retains its reservation until explicit cleanup or parent teardown.
    pub cleanup: Option<ChildLease>,
}

struct Operation<'a>(&'a Cell<bool>);

/// Single UI-thread owner. Backend callbacks run with every internal RefCell guard released.
pub struct WindowLifecycle<C: NativeChild> {
    authority: Rc<Authority>,
    limits: LifecycleLimits,
    clock: Box<dyn MonotonicClock>,
    wake: Waker,
    busy: Cell<bool>,
    state: RefCell<State<C>>,
}

mod access;
#[cfg(test)]
mod cleanup;
mod transitions;

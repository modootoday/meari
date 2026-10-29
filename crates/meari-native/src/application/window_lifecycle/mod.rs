//! Native-child ownership ports and UI-thread coordination; no OS or product policy.
mod authority;
mod completion;
mod coordinator;
mod ports;
mod state;
pub use crate::domain::window_lifecycle::*;
pub use authority::ChildLease;
pub use completion::CompletionSignal;
pub use coordinator::{RegistrationFailure, RegistrationProblem, WindowLifecycle};
pub use ports::{ChildCreationFailure, CleanupRequest, MonotonicClock, NativeChild};

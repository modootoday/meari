//! UI-thread document ownership and explicit host turns. Window integration is an adapter.
mod children;
mod handle;
mod hooks;
mod retained_observation;
pub use hooks::NativeHookFailure;
pub use retained_observation::{RetainedCommitProof, RetainedFrameProof};
mod runtime;
mod scheduling;
mod types;
pub use children::{ChildCloseCheckpoint, ChildHostError, ChildWindowHost};
pub use handle::NativeDocumentHandle;
pub use runtime::{NativeHostRuntime, NativeWindowHost};
#[cfg(feature = "window")]
pub(crate) use scheduling::HostAttemptError;
pub use scheduling::{HostRetry, HostScheduleLimits};
pub use types::*;
#[cfg(feature = "window")]
pub(super) mod access;
#[cfg(feature = "window")]
pub(crate) mod window_port;

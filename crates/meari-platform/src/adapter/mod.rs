//! Per-OS adapters. OS-specific code lives only here; selection lives only in [`crate::CurrentOs`].
pub mod macos;
pub mod unix;
pub mod windows;

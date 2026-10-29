//! Per-OS adapters. OS-specific code lives only here; selection lives only in the crate root.
pub mod macos;
pub mod portable;
pub mod unix;
pub mod windows;

//! Grouped submodules; re-exported by the parent to keep their paths.

#[cfg(all(feature = "gl", windows))]
pub mod gl_suite;
pub mod screen_suite;
pub mod styles_suite;

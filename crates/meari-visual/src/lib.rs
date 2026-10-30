//! Visual regression harness library: one TSX fixture set, two renderers, one pixel diff.
//! Shared by the `meari-visual` binary and the side-by-side demo (`crates/meari-demo`).
//!
//! Native: `meari-native` InteractiveDocument -> blitz layout -> vello CPU raster (offscreen, no window).
//! Web: the same portable DOM plan the WASM `WebDocument` adapter mounts (`bind_with_scope` ->
//! `StateProgram::project` -> `dom::prepare`), serialized to HTML and screenshotted by headless Chrome.
pub mod compare;
pub mod fixtures;
pub mod fonts;
pub mod host;
pub mod render;
pub mod report;
pub mod scroll;
// Contact-sheet writer lives with the report it illustrates; path kept for callers.
pub use report::sheet;

pub const WIDTH: u32 = 480;
pub const HEIGHT: u32 = 320;

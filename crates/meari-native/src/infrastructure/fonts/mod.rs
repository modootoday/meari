//! Pinned Noto font files (SIL OFL-1.1): download, verification, cache, Web slices, catalog.
//!
//! No font bytes are compiled into any binary. `meari fonts fetch` / `cargo xtask fonts-fetch`
//! downloads the manifest of `meari::domain::design::fonts` into `runtime/fonts/noto/<rev>/`;
//! everything else reads that cache and verifies length + sha256 on every load.
mod cache;
mod catalog;
mod web;

pub use cache::{CACHE_ENV, FontCache, hex_sha256, verify};
pub use catalog::{FontCatalogError, PreparedFontCatalog, prepare_font_catalog};
pub use web::{WebSlice, web_slices};

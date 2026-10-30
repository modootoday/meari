//! Pinned Noto font downloads for the visual test: the manifest lives in
//! `meari::domain::design::fonts`, download/verify/cache in `meari_native::infrastructure::fonts`
//! (cache `runtime/fonts/noto/<revision>/`; a valid cache works offline; a length or checksum
//! mismatch is an error, never skipped). No font bytes are compiled into any binary.
pub use meari::domain::design::fonts::{MANIFEST, Pin, REVISION, select};
use meari_native::infrastructure::fonts::FontCache;
pub use meari_native::infrastructure::fonts::verify;
use std::path::{Path, PathBuf};

/// Scripts the visual test needs: FONT_PIN (Noto Sans/Serif/Mono, Latin), the canonical-font
/// fixture (Hangul, Japanese) and the RTL/complex-script fixtures.
pub const VISUAL_TEST_SCRIPTS: &[&str] = &[
    "latin",
    "hangul",
    "kana",
    "arabic",
    "hebrew",
    "devanagari",
    "thai",
];

pub fn cache(repo_root: &Path) -> FontCache {
    FontCache::at(repo_root.join("runtime/fonts"))
}

/// Downloads (once) and verifies the selected scripts under `<repo_root>/runtime/fonts`.
pub fn ensure(scripts: &[&str], repo_root: &Path) -> Result<Vec<(Pin, PathBuf)>, String> {
    cache(repo_root).ensure(scripts)
}

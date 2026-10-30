//! Download + verify + cache of the pinned Noto files, like the pinned Tailwind binary.
use meari::domain::design::fonts::{Pin, REVISION, select};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Environment variable naming the font cache root (default: `runtime/fonts` found from the
/// current directory upwards, else `./runtime/fonts`).
pub const CACHE_ENV: &str = "MEARI_FONT_CACHE";

/// `runtime/fonts` root of the verified font cache.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontCache {
    root: PathBuf,
}

impl FontCache {
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `$MEARI_FONT_CACHE`, else the nearest `runtime/fonts` above the current directory,
    /// else `./runtime/fonts` (created on first fetch).
    pub fn default_location() -> Self {
        if let Some(root) = std::env::var_os(CACHE_ENV).filter(|v| !v.is_empty()) {
            return Self::at(PathBuf::from(root));
        }
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let found = cwd
            .ancestors()
            .map(|dir| dir.join("runtime/fonts"))
            .find(|dir| dir.is_dir());
        Self::at(found.unwrap_or_else(|| cwd.join("runtime/fonts")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Directory of the downloaded files of the pinned revision.
    pub fn noto_dir(&self) -> PathBuf {
        self.root.join("noto").join(&REVISION[..12])
    }

    pub fn path(&self, pin: &Pin) -> PathBuf {
        self.noto_dir().join(pin.file)
    }

    pub fn notice_path(&self, pin: &Pin) -> PathBuf {
        self.noto_dir().join(format!("{}-OFL.txt", pin.dir))
    }

    /// Ensures every selected file and its OFL notice is cached and verified, downloading only
    /// what is missing. Offline with a valid cache succeeds; any mismatch is an error.
    pub fn ensure(&self, scripts: &[&str]) -> Result<Vec<(Pin, PathBuf)>, String> {
        let dir = self.noto_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut out = Vec::new();
        for pin in select(scripts)? {
            let path = self.path(&pin);
            fetch_verified(&pin.url(), &path, pin.length, pin.sha256)?;
            fetch_verified(
                &pin.ofl_url(),
                &self.notice_path(&pin),
                pin.ofl_length,
                pin.ofl_sha256,
            )?;
            out.push((pin, path));
        }
        Ok(out)
    }

    /// Verified bytes of one cached file; never downloads. Missing files name the fetch command.
    pub fn load(&self, pin: &Pin) -> Result<Vec<u8>, String> {
        let path = self.path(pin);
        let bytes = std::fs::read(&path).map_err(|e| missing(pin, &path, e))?;
        verify(&path.display().to_string(), &bytes, pin.length, pin.sha256)?;
        Ok(bytes)
    }

    /// Verified OFL notice of one cached file.
    pub fn notice(&self, pin: &Pin) -> Result<String, String> {
        let path = self.notice_path(pin);
        let bytes = std::fs::read(&path).map_err(|e| missing(pin, &path, e))?;
        verify(
            &path.display().to_string(),
            &bytes,
            pin.ofl_length,
            pin.ofl_sha256,
        )?;
        String::from_utf8(bytes).map_err(|e| format!("{}: {e}", path.display()))
    }
}

fn missing(pin: &Pin, path: &Path, e: std::io::Error) -> String {
    format!(
        "font {} not in the cache ({}: {e}); run `meari fonts fetch {}` (or `cargo xtask fonts-fetch {}`) once with network access",
        pin.family,
        path.display(),
        pin.script,
        pin.script
    )
}

/// Checks `bytes` against a pin; the error names the expected and actual values.
pub fn verify(name: &str, bytes: &[u8], length: u64, sha256: &str) -> Result<(), String> {
    let actual = hex_sha256(bytes);
    if bytes.len() as u64 != length || actual != sha256 {
        return Err(format!(
            "font checksum mismatch for {name}: expected {length} bytes sha256 {sha256}, got {} bytes sha256 {actual}; delete the file and run `meari fonts fetch` again",
            bytes.len()
        ));
    }
    Ok(())
}

pub fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn fetch_verified(url: &str, path: &Path, length: u64, sha256: &str) -> Result<(), String> {
    let name = path.display().to_string();
    if let Ok(bytes) = std::fs::read(path) {
        return verify(&name, &bytes, length, sha256);
    }
    let part = path.with_extension("part");
    let status = std::process::Command::new("curl")
        .args(["-fsSL", "--retry", "2", "-o"])
        .arg(&part)
        .arg(url)
        .status()
        .map_err(|e| format!("cannot run curl to fetch {url}: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&part);
        return Err(format!(
            "font download failed ({status}) for {url} and no cached copy at {name} (offline?)"
        ));
    }
    let bytes = std::fs::read(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    if let Err(e) = verify(url, &bytes, length, sha256) {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, path).map_err(|e| format!("{name}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use meari::domain::design::fonts::MANIFEST;

    #[test]
    fn checksum_mismatch_is_a_clear_error() {
        let err = verify("x.ttf", b"abc", 3, MANIFEST[0].sha256).unwrap_err();
        assert!(err.contains("checksum mismatch for x.ttf"), "{err}");
        verify(
            "abc",
            b"abc",
            3,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .unwrap();
    }

    #[test]
    fn missing_file_names_the_fetch_command() {
        let cache = FontCache::at(std::env::temp_dir().join("meari-font-cache-empty-test"));
        let err = cache.load(&MANIFEST[0]).unwrap_err();
        assert!(err.contains("meari fonts fetch latin"), "{err}");
    }

    #[test]
    fn corrupted_cache_file_is_an_error_not_a_skip() {
        let root =
            std::env::temp_dir().join(format!("meari-font-cache-bad-{}", std::process::id()));
        let cache = FontCache::at(&root);
        std::fs::create_dir_all(cache.noto_dir()).unwrap();
        std::fs::write(cache.path(&MANIFEST[0]), b"not a font").unwrap();
        let err = cache.ensure(&["latin"]).unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");
        let _ = std::fs::remove_dir_all(root);
    }
}

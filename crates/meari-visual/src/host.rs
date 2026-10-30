//! Host defaults for the harness: headless Chrome and the pinned Tailwind standalone binary per
//! OS family (selected through `meari-platform`, no OS `cfg` here), and OS-neutral `file://` URLs.
use meari_platform::{OsFamily, PlatformHost};
use std::path::{Path, PathBuf};

/// Default Chrome executable: the first existing well-known install path for this OS family.
pub fn default_chrome() -> String {
    let candidates: Vec<PathBuf> = match meari_platform::current().os_family() {
        OsFamily::Windows => {
            let mut roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
                .iter()
                .filter_map(|var| std::env::var_os(var).map(PathBuf::from))
                .collect();
            roots.push(PathBuf::from(r"C:\Program Files"));
            roots
                .into_iter()
                .map(|root| root.join(r"Google\Chrome\Application\chrome.exe"))
                .collect()
        }
        OsFamily::MacOs => vec![PathBuf::from(
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        )],
        OsFamily::Unix => vec![PathBuf::from("/usr/bin/google-chrome")],
    };
    let first = candidates[0].clone();
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or(first)
        .to_string_lossy()
        .into_owned()
}

/// How pixel mismatches gate the run on this host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelGate {
    /// Reference host (Linux): any supported-fixture mismatch fails the run.
    Strict,
    /// Windows/macOS: glyph anti-aliasing follows the OS font renderer and user settings
    /// (ClearType, gamma), so an AA-only mismatch is reported, not failed. Layout
    /// differences and render errors still fail.
    Advisory,
}

impl PixelGate {
    pub fn for_os(os: OsFamily) -> Self {
        match os {
            OsFamily::Unix => PixelGate::Strict,
            OsFamily::Windows | OsFamily::MacOs => PixelGate::Advisory,
        }
    }

    /// Gate for the OS this binary runs on (selected by `meari-platform`).
    pub fn current() -> Self {
        Self::for_os(meari_platform::current().os_family())
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "strict" => Some(PixelGate::Strict),
            "advisory" => Some(PixelGate::Advisory),
            _ => None,
        }
    }
}

/// Default pinned Tailwind standalone binary under `runtime/tools/tailwind/` for this OS family.
pub fn default_tailwind(repo_root: &Path) -> PathBuf {
    let asset = match meari_platform::current().os_family() {
        OsFamily::Windows => "tailwindcss-windows-x64.exe",
        OsFamily::MacOs => "tailwindcss-macos-arm64",
        OsFamily::Unix => "tailwindcss-linux-x64",
    };
    repo_root.join("runtime/tools/tailwind").join(asset)
}

/// `file://` URL for a local path on any OS (`/a/b` -> `file:///a/b`, `C:\a b` -> `file:///C:/a%20b`).
pub fn file_url(path: &Path) -> String {
    let absolute;
    let raw = path.to_string_lossy();
    let rooted = raw.starts_with(['/', '\\']) || raw.contains(':');
    let path = if path.is_relative() && !rooted {
        absolute = std::env::current_dir()
            .map(|dir| dir.join(path))
            .unwrap_or(path.into());
        absolute.as_path()
    } else {
        path
    };
    let text = path.to_string_lossy().replace('\\', "/");
    let text = text.strip_prefix("//?/").unwrap_or(&text);
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

/// Linux Chrome takes text anti-aliasing from the host fontconfig `rgba`: GitHub's
/// ubuntu-24.04 runner reports `rgb` (subpixel LCD colour fringes), the box baseline `none`
/// (grayscale). Pin grayscale by layering a config over the host's (`$FONTCONFIG_FILE` or
/// `/etc/fonts/fonts.conf`), so every Linux host renders the reference the same way.
/// (`--disable-lcd-text` is not equivalent: it also changes text under opacity layers.)
/// Returns the config path to pass as `FONTCONFIG_FILE`, or `None` off Linux/BSD.
pub fn chrome_fontconfig(dir: &Path) -> Result<Option<PathBuf>, String> {
    if meari_platform::current().os_family() != OsFamily::Unix {
        return Ok(None);
    }
    let base = std::env::var("FONTCONFIG_FILE").unwrap_or_else(|_| "/etc/fonts/fonts.conf".into());
    let path = dir.join("chrome-grayscale-fonts.conf");
    let body = format!(
        "<?xml version=\"1.0\"?>\n<!DOCTYPE fontconfig SYSTEM \"urn:fontconfig:fonts.dtd\">\n\
<fontconfig>\n<include ignore_missing=\"no\">{base}</include>\n\
<match target=\"font\"><edit name=\"rgba\" mode=\"assign\"><const>none</const></edit>\
<edit name=\"antialias\" mode=\"assign\"><bool>true</bool></edit></match>\n</fontconfig>\n"
    );
    std::fs::write(&path, body).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::{OsFamily, PixelGate, file_url};

    #[test]
    fn linux_is_the_strict_reference_gate() {
        assert_eq!(PixelGate::for_os(OsFamily::Unix), PixelGate::Strict);
        assert_eq!(PixelGate::for_os(OsFamily::Windows), PixelGate::Advisory);
        assert_eq!(PixelGate::for_os(OsFamily::MacOs), PixelGate::Advisory);
        assert_eq!(PixelGate::parse("strict"), Some(PixelGate::Strict));
        assert_eq!(PixelGate::parse("loose"), None);
    }
    use std::path::Path;

    #[test]
    fn file_urls_are_os_neutral() {
        assert_eq!(
            file_url(Path::new("/tmp/a b.html")),
            "file:///tmp/a%20b.html"
        );
        assert_eq!(
            file_url(Path::new(r"C:\workspaces\meari\x.html")),
            "file:///C:/workspaces/meari/x.html"
        );
        assert_eq!(
            file_url(Path::new(r"\\?\C:\w\x.woff2")),
            "file:///C:/w/x.woff2"
        );
    }
}

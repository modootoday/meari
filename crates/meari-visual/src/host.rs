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

/// Linux Chrome takes its text rasterization parameters (anti-aliasing, hinting, subpixel
/// order, LCD filter, embedded bitmaps) from the host fontconfig, matched by family name.
/// Layering over the host config was not enough: GitHub's ubuntu-24.04 runner and the box
/// differ in conf.d rules and installed fonts, and the same Chrome 156.0.8078.47 drew the
/// same fixtures with different glyph coverage (heading-text 2443 px apart), failing Strict.
/// So the reference uses a self-contained config: only the pinned Noto cache as font
/// directory (the faces the fixtures load) and every render parameter assigned explicitly
/// to the box baseline the tolerances were measured on (grayscale, slight hinting).
/// (`--disable-lcd-text` is not equivalent: it also changes text under opacity layers.)
/// Returns the config path to pass as `FONTCONFIG_FILE`, or `None` off Linux/BSD.
pub fn chrome_fontconfig(dir: &Path) -> Result<Option<PathBuf>, String> {
    if meari_platform::current().os_family() != OsFamily::Unix {
        return Ok(None);
    }
    let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/fonts");
    let path = dir.join("chrome-grayscale-fonts.conf");
    std::fs::write(
        &path,
        fontconfig_body(&fonts, &dir.join("fontconfig-cache")),
    )
    .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path))
}

/// The host-independent fontconfig for the Chrome reference (see [`chrome_fontconfig`]).
fn fontconfig_body(fonts: &Path, cache: &Path) -> String {
    const RENDER: &str = "<edit name=\"antialias\" mode=\"assign\"><bool>true</bool></edit>\
<edit name=\"hinting\" mode=\"assign\"><bool>true</bool></edit>\
<edit name=\"hintstyle\" mode=\"assign\"><const>hintslight</const></edit>\
<edit name=\"autohint\" mode=\"assign\"><bool>false</bool></edit>\
<edit name=\"rgba\" mode=\"assign\"><const>none</const></edit>\
<edit name=\"lcdfilter\" mode=\"assign\"><const>lcddefault</const></edit>\
<edit name=\"embeddedbitmap\" mode=\"assign\"><bool>false</bool></edit>\
<edit name=\"embolden\" mode=\"assign\"><bool>false</bool></edit>";
    format!(
        "<?xml version=\"1.0\"?>\n<!DOCTYPE fontconfig SYSTEM \"urn:fontconfig:fonts.dtd\">\n\
<fontconfig>\n<dir>{}</dir>\n<cachedir>{}</cachedir>\n\
<match target=\"font\">{RENDER}</match>\n</fontconfig>\n",
        fonts.display(),
        cache.display()
    )
}

#[cfg(test)]
mod tests {
    use super::{OsFamily, PixelGate, file_url, fontconfig_body};

    #[test]
    fn chrome_fontconfig_is_host_independent() {
        let body = fontconfig_body(Path::new("/r/runtime/fonts"), Path::new("/o/fc"));
        assert!(
            !body.contains("<include"),
            "no host config may leak in: {body}"
        );
        assert!(body.contains("<dir>/r/runtime/fonts</dir>"));
        for (name, value) in [
            ("rgba", "<const>none</const>"),
            ("hintstyle", "<const>hintslight</const>"),
            ("antialias", "<bool>true</bool>"),
            ("embeddedbitmap", "<bool>false</bool>"),
        ] {
            assert!(body.contains(&format!("name=\"{name}\" mode=\"assign\">{value}")));
        }
    }

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

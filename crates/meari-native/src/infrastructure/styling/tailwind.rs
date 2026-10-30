//! Explicit adapter for a pinned, trusted upstream standalone compiler.
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

pub const VERSION: &str = "4.3.3";
pub const WINDOWS_X64_SHA256: &str =
    "e0e260ce048014e9268f6237ff18f8ccf02cef521cbd0ae04e82c2cdf7aa3955";

/// Official v4.3.3 `tailwindcss-linux-x64` release asset (matches upstream sha256sums.txt).
pub const LINUX_X64_SHA256: &str =
    "dc61b3ac6b8c9ca874c0cc4c57b2409791a64c5540404ca5f5367360babc313a";

/// Official v4.3.3 `tailwindcss-macos-arm64` release asset (matches upstream sha256sums.txt);
/// used by the macOS CI runner.
pub const MACOS_ARM64_SHA256: &str =
    "cdf646702987a743464dff4d9c60fd4480d1c1e73dd819a9a67f1078815dce9d";

pub struct TailwindCompiler {
    executable: PathBuf,
    artifact: String,
}

impl crate::application::StylesheetProjectCompiler for TailwindCompiler {
    fn compile_project(
        &self,
        project: &meari::domain::design::stylesheet::StylesheetProject,
        classes: &str,
        theme: &meari::domain::design::theme::Theme,
    ) -> Result<crate::application::CompiledStylesheet, String> {
        use meari::domain::design::stylesheet::StylesheetMode;
        if project.mode != StylesheetMode::Tailwind {
            return Err("Tailwind compiler requires tailwind mode".into());
        }
        super::super::stylesheet::validate(project)?;
        if project.bridge_theme {
            theme.validate()?;
        }
        let scratch = Scratch::new()?;
        for (name, css) in &project.files {
            let path = scratch.0.join("sources").join(name);
            fs::create_dir_all(path.parent().ok_or("missing CSS parent")?)
                .map_err(|e| e.to_string())?;
            fs::write(path, css).map_err(|e| e.to_string())?;
        }
        let profile = if project.bridge_theme {
            theme_css(theme)
        } else {
            String::new()
        };
        let input = format!(
            "{profile}\n@import {};\n@source inline({});\n",
            css_string(&format!("./sources/{}", project.entry)),
            css_string(classes)
        );
        let css = self.run(&scratch.0, &input)?;
        Ok(super::super::stylesheet::receipt(
            project,
            css,
            &format!("tailwindcss-{VERSION}-{}", self.artifact),
        ))
    }
}

impl crate::application::StylesheetCompiler for TailwindCompiler {
    fn compile(
        &self,
        classes: &str,
        theme: &meari::domain::design::theme::Theme,
    ) -> Result<String, String> {
        self.compile(classes, theme)
    }
}

impl TailwindCompiler {
    pub fn windows_x64(executable: &Path) -> Result<Self, String> {
        Self::verified(executable, &[("windows-x64", WINDOWS_X64_SHA256)])
    }

    pub fn linux_x64(executable: &Path) -> Result<Self, String> {
        Self::verified(executable, &[("linux-x64", LINUX_X64_SHA256)])
    }

    /// Accepts any pinned v4.3.3 standalone artifact (Windows x64, Linux x64 or macOS arm64).
    pub fn pinned(executable: &Path) -> Result<Self, String> {
        Self::verified(
            executable,
            &[
                ("windows-x64", WINDOWS_X64_SHA256),
                ("linux-x64", LINUX_X64_SHA256),
                ("macos-arm64", MACOS_ARM64_SHA256),
            ],
        )
    }

    fn verified(executable: &Path, pins: &[(&str, &'static str)]) -> Result<Self, String> {
        let executable = executable.canonicalize().map_err(|e| e.to_string())?;
        let bytes = fs::read(&executable).map_err(|e| e.to_string())?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        let artifact = pins
            .iter()
            .find(|(_, pin)| *pin == hash)
            .map(|(name, pin)| format!("{name}/{pin}"))
            .ok_or("Tailwind executable hash does not match a pinned v4.3.3 artifact")?;
        Ok(Self {
            executable,
            artifact,
        })
    }

    /// Backwards-compatible generated profile input. Custom CSS uses compile_project.
    pub fn compile(
        &self,
        classes: &str,
        theme: &meari::domain::design::theme::Theme,
    ) -> Result<String, String> {
        theme.validate()?;
        let scratch = Scratch::new()?;
        let theme_css = theme_css(theme);
        let input = format!(
            "@import \"tailwindcss\" source(none);\n@custom-variant dark (&:is(.dark *));\n@source inline({});\n{theme_css}\n",
            css_string(classes)
        );
        self.run(&scratch.0, &input)
    }

    fn run(&self, directory: &Path, input: &str) -> Result<String, String> {
        fs::write(directory.join("input.css"), input).map_err(|e| e.to_string())?;
        let output = Command::new(&self.executable)
            .current_dir(directory)
            // The standalone (Bun) binary aborts when TMP/TEMP point at a missing directory.
            .env("TMPDIR", directory)
            .env("TMP", directory)
            .env("TEMP", directory)
            .args(["--input", "input.css", "--output", "output.css", "--minify"])
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "Tailwind {VERSION} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        fs::read_to_string(directory.join("output.css")).map_err(|e| e.to_string())
    }
}

fn theme_css(theme: &meari::domain::design::theme::Theme) -> String {
    use meari::domain::design::utilities::COLORS;
    use std::fmt::Write;
    let mut css = format!(
        "@theme inline {{ --spacing: {}rem; --radius-sm: {}rem; --radius-md: {}rem; --radius-lg: {}rem;",
        theme.spacing_rem,
        theme.radius_rem * 0.6,
        theme.radius_rem * 0.8,
        theme.radius_rem
    );
    for name in COLORS {
        write!(css, "--color-{name}:var(--{name});").unwrap();
    }
    css.push('}');
    for (selector, colors) in [(":root", &theme.light), (".dark", &theme.dark)] {
        write!(css, "{selector}{{").unwrap();
        for name in COLORS {
            let [r, g, b, a] = colors[*name];
            write!(css, "--{name}:rgba({r},{g},{b},{});", f32::from(a) / 255.0).unwrap();
        }
        css.push('}');
    }
    // Canonical font stacks (non-inline, so `font-sans` stays `var(--font-sans)`).
    write!(
        css,
        "@theme{{--font-sans:{};--font-serif:{};--font-mono:{};}}",
        meari::domain::design::fonts::SANS_FONT_FAMILY,
        meari::domain::design::fonts::SERIF_FONT_FAMILY,
        meari::domain::design::fonts::MONO_FONT_FAMILY
    )
    .unwrap();
    css.push_str("@layer base{html,body{background:var(--background);color:var(--foreground)}body{min-height:100vh}}");
    css
}

fn css_string(value: &str) -> String {
    let mut escaped = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            ch if ch.is_control() => escaped.push_str(&format!("\\{:x} ", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "bridge-tailwind-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).map_err(|e| e.to_string())?;
        Ok(Self(directory))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

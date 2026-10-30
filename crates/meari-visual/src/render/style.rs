//! Stylesheet resolution: fixture CSS verbatim, or Tailwind classes compiled by the pinned binary.
use crate::fixtures::Fixture;
use meari_native::infrastructure::tailwind::TailwindCompiler;
use std::path::Path;

/// Environment pin, applied identically to both renderers: generic families (`sans-serif`,
/// Tailwind's `ui-sans-serif, system-ui, ...`) resolve to different faces in blitz/fontique and
/// Chrome on this box, which is tracked separately by the `gaps/generic-font-family` fixture.
pub const FONT_PIN: &str = ":root{--default-font-family:\"Noto Sans\";--font-sans:\"Noto Sans\"}";

static PIN_FACES: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Gives the downloaded, verified Noto files (see `crate::fonts`) to Chrome as `@font-face`
/// rules on the same files native registers from the cache (author faces win over any installed
/// face of the same name). Call once at start-up.
pub fn use_pinned_fonts(files: &[(crate::fonts::Pin, std::path::PathBuf)]) -> Result<(), String> {
    let mut faces = String::new();
    for (pin, path) in files {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        crate::fonts::verify(&path.display().to_string(), &bytes, pin.length, pin.sha256)?;
        faces.push_str(&face_rule(pin, path));
    }
    let _ = PIN_FACES.set(faces);
    Ok(())
}

fn face_rule(pin: &crate::fonts::Pin, path: &Path) -> String {
    format!(
        "@font-face{{font-family:\"{}\";src:url(\"{}\") format(\"truetype\");font-style:{};font-weight:{};font-display:block}}",
        pin.family,
        crate::host::file_url(path),
        pin.style,
        pin.weight()
    )
}

fn pin_faces() -> &'static str {
    PIN_FACES.get().map(String::as_str).unwrap_or("")
}

/// Marker comment: the fixture renders with the canonical font stacks instead of FONT_PIN.
pub const CANONICAL_MARKER: &str = "// meari-visual: canonical-fonts";

static CANONICAL_FACES: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// `@font-face` rules for the canonical UI faces, served to Chrome as the WOFF2 unicode-range
/// slices the Web build ships (generated from the cached Noto TTF). Native resolves the same
/// faces from the cached TTF (registered by the font context); it has no WOFF2 decoder.
/// Call once at start-up after the cache is ensured.
pub fn use_canonical_faces(repo_root: &Path) -> Result<(), String> {
    use meari::domain::design::fonts::{UI_SCRIPTS, select};
    let cache = crate::fonts::cache(repo_root);
    let mut rules = String::new();
    for pin in select(UI_SCRIPTS)? {
        for slice in meari_native::infrastructure::fonts::web_slices(&cache, &pin)? {
            rules.push_str(&format!(
                "@font-face{{font-family:\"{}\";src:url(\"{}\") format(\"woff2\");font-style:{};font-weight:{};font-display:block;unicode-range:{}}}",
                pin.family,
                crate::host::file_url(&slice.path),
                pin.style,
                pin.weight(),
                slice.unicode_range
            ));
        }
    }
    let _ = CANONICAL_FACES.set(rules);
    Ok(())
}

pub fn canonical_faces() -> &'static str {
    CANONICAL_FACES.get().map(String::as_str).unwrap_or("")
}

/// Chrome side of the generic-family mapping: Chrome resolves `sans-serif` / `serif` /
/// `monospace` through OS preferences (Windows Arial, Linux fontconfig) that CSS cannot
/// change, so the harness rewrites those keywords to the pinned Noto families the native
/// comparison profile puts first for the same generics. Quoted strings, custom-property names
/// and identifiers such as `ui-sans-serif` or `.font-serif` are left alone.
pub fn chrome_generics(css: &str) -> String {
    const MAP: [(&str, &str); 3] = [
        ("sans-serif", "\"Noto Sans\""),
        ("monospace", "\"Noto Sans Mono\""),
        ("serif", "\"Noto Serif\""),
    ];
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    let mut out = String::with_capacity(css.len());
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i < css.len() {
        let rest = &css[i..];
        let c = rest.chars().next().unwrap();
        if let Some(q) = quote {
            if c == '\\' {
                let n = rest.chars().nth(1).map_or(0, char::len_utf8);
                out.push_str(&rest[..1 + n]);
                i += 1 + n;
                continue;
            }
            if c == q {
                quote = None;
            }
        } else if c == '"' || c == '\'' {
            quote = Some(c);
        } else if !css[..i].chars().next_back().is_some_and(ident)
            && let Some((word, noto)) = MAP.iter().find(|(word, _)| {
                rest.starts_with(word) && !rest[word.len()..].chars().next().is_some_and(ident)
            })
        {
            out.push_str(noto);
            i += word.len();
            continue;
        }
        out.push(c);
        i += c.len_utf8();
    }
    out
}

pub fn resolve(fixture: &Fixture, tailwind: &Path) -> Result<String, String> {
    let canonical = fixture.source.contains(CANONICAL_MARKER);
    if let Some(css) = &fixture.css {
        return Ok(if canonical {
            format!("{}\n{css}", canonical_faces())
        } else {
            format!("{}\n{css}", pin_faces())
        });
    }
    let compiler = TailwindCompiler::pinned(tailwind)
        .map_err(|e| format!("tailwind binary {}: {e}", tailwind.display()))?;
    let component = meari::parse(&fixture.source, meari::application::Language::Tsx)
        .map_err(|d| format!("parse: {}", d.message))?;
    let theme: meari::domain::design::theme::Theme = serde_json::from_str(
        &std::fs::read_to_string(crate::fixtures::dir().join("theme.json"))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("theme.json: {e}"))?;
    let css = meari_native::application::stylesheet(&component, &theme, &compiler)?;
    if canonical {
        return Ok(format!("{}\n{css}", canonical_faces()));
    }
    Ok(format!("{}\n{css}\n{FONT_PIN}", pin_faces()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_generics_maps_only_generic_keywords() {
        assert_eq!(
            chrome_generics(
                ".font-serif{font-family:serif}p{font:20px sans-serif;--x:'serif'}code{font-family:ui-monospace,monospace}"
            ),
            ".font-serif{font-family:\"Noto Serif\"}p{font:20px \"Noto Sans\";--x:'serif'}code{font-family:ui-monospace,\"Noto Sans Mono\"}"
        );
    }

    #[test]
    fn face_rule_points_chrome_at_the_cached_file() {
        let pin = crate::fonts::select(&["latin"]).unwrap()[1];
        let rule = face_rule(&pin, Path::new("/c/NotoSans-Italic.ttf"));
        assert!(rule.contains("font-family:\"Noto Sans\""), "{rule}");
        assert!(
            rule.contains("font-style:italic;font-weight:100 900"),
            "{rule}"
        );
        assert!(rule.contains("file:///c/NotoSans-Italic.ttf"), "{rule}");
    }
}

//! Stylesheet resolution: fixture CSS verbatim, or Tailwind classes compiled by the pinned binary.
use crate::fixtures::Fixture;
use meari_native::infrastructure::tailwind::TailwindCompiler;
use std::path::Path;

/// Environment pin, applied identically to both renderers: generic families (`sans-serif`,
/// Tailwind's `ui-sans-serif, system-ui, ...`) resolve to different faces in blitz/fontique and
/// Chrome on this box, which is tracked separately by the `gaps/generic-font-family` fixture.
pub const FONT_PIN: &str = ":root{--default-font-family:\"Noto Sans\";--font-sans:\"Noto Sans\"}";

static PIN_FACES: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Gives the downloaded, verified Noto files (see `crate::fonts`) to both renderers: native
/// registers the bytes; Chrome gets `@font-face` rules on the same files (author faces win
/// over any installed face of the same name). Call once at start-up.
pub fn use_pinned_fonts(files: &[(crate::fonts::Pin, std::path::PathBuf)]) -> Result<(), String> {
    let mut faces = String::new();
    for (pin, path) in files {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        crate::fonts::verify(&path.display().to_string(), &bytes, pin.length, pin.sha256)?;
        meari_native::infrastructure::engine::generic_fonts::register_comparison_font(Box::leak(
            bytes.into_boxed_slice(),
        ));
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

/// Marker comment: the fixture renders with the canonical bundled fonts instead of FONT_PIN.
pub const CANONICAL_MARKER: &str = "// meari-visual: canonical-fonts";

/// `@font-face` rules for the bundled faces, served to Chrome as the same WOFF2 files the
/// Web build ships (upstream WOFF2, or the renamed `Meari Serif KR` unicode-range slices).
/// Native resolves the same faces from the bundled upstream TTF/OTF (registered by the font
/// context); its blitz build has no WOFF2 decoder.
pub fn canonical_faces() -> String {
    use meari::{application::fonts::web_files, domain::design::fonts::UI_FACES};
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../meari");
    UI_FACES
        .iter()
        .flat_map(|face| web_files(*face).into_iter().map(move |web| (*face, web)))
        .map(|(face, web)| {
            let range = web
                .unicode_range
                .map(|range| format!(";unicode-range:{range}"))
                .unwrap_or_default();
            format!(
                "@font-face{{font-family:\"{}\";src:url(\"{}\") format(\"woff2\");font-style:normal;font-weight:{};font-display:block{range}}}",
                face.web_family(),
                crate::host::file_url(&std::path::Path::new(root).join(web.file.path)),
                face.weight()
            )
        })
        .collect()
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

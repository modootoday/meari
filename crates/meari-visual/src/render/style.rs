//! Stylesheet resolution: fixture CSS verbatim, or Tailwind classes compiled by the pinned binary.
use crate::fixtures::Fixture;
use meari_native::infrastructure::tailwind::TailwindCompiler;
use std::path::Path;

/// Environment pin, applied identically to both renderers: generic families (`sans-serif`,
/// Tailwind's `ui-sans-serif, system-ui, ...`) resolve to different faces in blitz/fontique and
/// Chrome on this box, which is tracked separately by the `gaps/generic-font-family` fixture.
pub const FONT_PIN: &str = ":root{--default-font-family:\"Noto Sans\";--font-sans:\"Noto Sans\"}";

/// Marker comment: the fixture renders with the canonical bundled fonts instead of FONT_PIN.
pub const CANONICAL_MARKER: &str = "// meari-visual: canonical-fonts";

/// `@font-face` rules for the bundled faces, served to Chrome as the same upstream WOFF2 the
/// Web build ships. Native resolves the same families from the bundled upstream TTF
/// (registered by the font context); its blitz build has no WOFF2 decoder.
pub fn canonical_faces() -> String {
    use meari::domain::design::fonts::{FontFormat, UI_FACES};
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../meari");
    UI_FACES
        .iter()
        .map(|face| {
            format!(
                "@font-face{{font-family:\"{}\";src:url(\"file://{root}/{}\") format(\"woff2\");font-style:normal;font-weight:{};font-display:block}}",
                face.family(),
                face.file(FontFormat::Woff2).path,
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
            css.clone()
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
    Ok(format!("{css}\n{FONT_PIN}"))
}

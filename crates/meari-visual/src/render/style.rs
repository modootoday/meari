//! Stylesheet resolution: fixture CSS verbatim, or Tailwind classes compiled by the pinned binary.
use crate::fixtures::Fixture;
use meari_native::infrastructure::tailwind::TailwindCompiler;
use std::path::Path;

/// Environment pin, applied identically to both renderers: generic families (`sans-serif`,
/// Tailwind's `ui-sans-serif, system-ui, ...`) resolve to different faces in blitz/fontique and
/// Chrome on this box, which is tracked separately by the `gaps/generic-font-family` fixture.
pub const FONT_PIN: &str = ":root{--default-font-family:\"Noto Sans\";--font-sans:\"Noto Sans\"}";

pub fn resolve(fixture: &Fixture, tailwind: &Path) -> Result<String, String> {
    if let Some(css) = &fixture.css {
        return Ok(css.clone());
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
    Ok(format!("{css}\n{FONT_PIN}"))
}

//! Retained font assets over the verified resource API, read from the pinned cache.
use super::{FontCache, web_slices};
use meari::application::assets::VerifiedAssets;
use meari::domain::{
    assets::{AssetDescriptor, AssetManifest},
    build::{BuildPath, ContentHash, InputRole},
    design::fonts::{
        FontRequirement, FontSupport, MONO_FONT_FAMILY, Pin, SANS_FONT_FAMILY, SERIF_FONT_FAMILY,
        UI_FONT_FAMILY, UI_SCRIPTS, select,
    },
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontCatalogError {
    UnsupportedRequirement(FontRequirement),
    /// Missing or mismatching cache file; the message names `meari fonts fetch`.
    Cache(String),
    Assets(String),
}

pub struct PreparedFontCatalog {
    /// Native resources: the pinned upstream TTF.
    pub assets: VerifiedAssets,
    /// Web resources: WOFF2 unicode-range slices generated from the same TTF.
    pub web_assets: VerifiedAssets,
    /// Web `@font-face` rules: WOFF2 only, `font-display:swap`.
    pub css: String,
    /// Native `@font-face` rules over the same faces' TTF.
    pub native_css: String,
    /// Complete OFL notice of every delivered family, keyed by family.
    pub licenses: BTreeMap<&'static str, String>,
    pub support: FontSupport,
}

/// Canonical UI faces (Noto Sans/Serif, KR, JP, upright and italic) from `cache`. Never
/// downloads: a missing file is `FontCatalogError::Cache` naming the fetch command.
pub fn prepare_font_catalog(
    cache: &FontCache,
    requirement: FontRequirement,
) -> Result<PreparedFontCatalog, FontCatalogError> {
    if requirement != FontRequirement::PinnedAssets {
        return Err(FontCatalogError::UnsupportedRequirement(requirement));
    }
    let mut files = [BTreeMap::new(), BTreeMap::new()];
    let mut blobs = [BTreeMap::new(), BTreeMap::new()];
    let mut css = String::new();
    let mut native_css = String::new();
    let mut licenses = BTreeMap::new();
    for pin in select(UI_SCRIPTS).map_err(FontCatalogError::Cache)? {
        let native = cache.load(&pin).map_err(FontCatalogError::Cache)?;
        licenses.insert(
            pin.family,
            cache.notice(&pin).map_err(FontCatalogError::Cache)?,
        );
        let mut deliveries = vec![(1, format!("fonts/noto/{}", pin.file), None, native)];
        for slice in web_slices(cache, &pin).map_err(FontCatalogError::Cache)? {
            let bytes = std::fs::read(&slice.path)
                .map_err(|e| FontCatalogError::Cache(format!("{}: {e}", slice.path.display())))?;
            deliveries.push((
                0,
                format!("fonts/web/{}", slice.name),
                Some(slice.unicode_range),
                bytes,
            ));
        }
        for (slot, source, range, bytes) in deliveries {
            let (media, format) = if slot == 0 {
                ("font/woff2", "woff2")
            } else {
                ("font/ttf", "truetype")
            };
            let hash = ContentHash::of(&bytes);
            let descriptor = AssetDescriptor {
                role: InputRole::Font,
                content: hash.clone(),
                length: bytes.len() as u64,
                media_type: media.into(),
                license: Some(format!("OFL-1.1; {}-OFL.txt", pin.dir)),
            };
            let output = descriptor.output_path().map_err(FontCatalogError::Assets)?;
            let range = range
                .map(|range| format!("unicode-range:{range};"))
                .unwrap_or_default();
            let rule = face_rule(&pin, output.as_str(), format, &range);
            if slot == 0 {
                css.push_str(&rule)
            } else {
                native_css.push_str(&rule)
            }
            let source = BuildPath::try_from(source).map_err(FontCatalogError::Assets)?;
            files[slot].insert(source, descriptor);
            blobs[slot].insert(hash, bytes);
        }
    }
    let root = format!(
        ":root{{--meari-font-ui:{UI_FONT_FAMILY};--font-sans:{SANS_FONT_FAMILY};--default-font-family:{SANS_FONT_FAMILY};--font-serif:{SERIF_FONT_FAMILY};--font-mono:{MONO_FONT_FAMILY};}}\n"
    );
    css.push_str(&root);
    native_css.push_str(&root);
    let [web_files, native_files] = files;
    let [web_blobs, native_blobs] = blobs;
    let verified = |files, blobs| {
        VerifiedAssets::new(
            AssetManifest {
                schema_version: 1,
                files,
            },
            blobs,
        )
        .map_err(FontCatalogError::Assets)
    };
    Ok(PreparedFontCatalog {
        assets: verified(native_files, native_blobs)?,
        web_assets: verified(web_files, web_blobs)?,
        css,
        native_css,
        licenses,
        support: FontSupport::PinnedAssetsOnly,
    })
}

fn face_rule(pin: &Pin, url: &str, format: &str, range: &str) -> String {
    format!(
        "@font-face{{font-family:\"{}\";src:url(\"/{url}\") format(\"{format}\");font-style:{};font-weight:{};{range}font-display:swap;}}\n",
        pin.family,
        pin.style,
        pin.weight(),
    )
}

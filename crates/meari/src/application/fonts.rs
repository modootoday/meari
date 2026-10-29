//! Explicitly selected retained font assets over the existing verified resource API.
use super::assets::VerifiedAssets;
use crate::domain::{
    assets::{AssetDescriptor, AssetManifest},
    build::{BuildPath, ContentHash, InputRole},
    design::fonts::{
        FontFace, FontFile, FontFormat, FontLicense, FontRequirement, FontSupport, LICENSES,
        SANS_FONT_FAMILY, SERIF_FONT_FAMILY, UI_FACES, UI_FONT_FAMILY,
    },
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontCatalogError {
    UnsupportedRequirement(FontRequirement),
    FontIntegrity(FontFace),
    LicenseIntegrity(FontLicense),
    Assets(String),
}

pub struct PreparedFontCatalog {
    /// Native resources: upstream TTF (native has no WOFF2 decoder).
    pub assets: VerifiedAssets,
    /// Web resources: upstream WOFF2 of the same faces.
    pub web_assets: VerifiedAssets,
    /// Web `@font-face` rules: upstream WOFF2, `font-display:swap`.
    pub css: String,
    /// Native `@font-face` rules over the same faces' upstream TTF.
    pub native_css: String,
    pub support: FontSupport,
}

impl PreparedFontCatalog {
    /// Complete, unchanged OFL notices remain separate from the binary asset manifest.
    pub fn license(&self, license: FontLicense) -> &'static str {
        license_text(license)
    }
}

pub fn prepare_font_catalog(
    requirement: FontRequirement,
) -> Result<PreparedFontCatalog, FontCatalogError> {
    if requirement != FontRequirement::PinnedAssets {
        return Err(FontCatalogError::UnsupportedRequirement(requirement));
    }
    for license in LICENSES {
        verify_license(license, license_text(license).as_bytes())?;
    }
    let mut files = [BTreeMap::new(), BTreeMap::new()];
    let mut blobs = [BTreeMap::new(), BTreeMap::new()];
    let mut css = String::new();
    let mut native_css = String::new();
    for face in UI_FACES {
        for format in [FontFormat::Woff2, FontFormat::Truetype] {
            let file = face.file(format);
            let bytes = face_bytes(face, format);
            verify_font(face, &file, bytes)?;
            let hash = ContentHash::of(bytes);
            let descriptor = AssetDescriptor {
                role: InputRole::Font,
                content: hash.clone(),
                length: file.length,
                media_type: format.media_type().into(),
                license: Some(format!("OFL-1.1; {}", face.license().path())),
            };
            let output = descriptor.output_path().map_err(FontCatalogError::Assets)?;
            let rule = format!(
                "@font-face{{font-family:\"{}\";src:url(\"/{}\") format(\"{}\");font-style:normal;font-weight:{};font-display:swap;}}\n",
                face.family(),
                output.as_str(),
                format.css(),
                face.weight(),
            );
            match format {
                FontFormat::Woff2 => css.push_str(&rule),
                FontFormat::Truetype => native_css.push_str(&rule),
            }
            let source =
                BuildPath::try_from(file.path.to_owned()).map_err(FontCatalogError::Assets)?;
            let slot = usize::from(format == FontFormat::Woff2);
            files[slot].insert(source, descriptor);
            blobs[slot].insert(hash, bytes.to_vec());
        }
    }
    let root = format!(
        ":root{{--meari-font-ui:{UI_FONT_FAMILY};--font-sans:{SANS_FONT_FAMILY};--default-font-family:{SANS_FONT_FAMILY};--font-serif:{SERIF_FONT_FAMILY};}}\n"
    );
    css.push_str(&root);
    native_css.push_str(&root);
    let [native_files, web_files] = files;
    let [native_blobs, web_blobs] = blobs;
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
        support: FontSupport::PinnedAssetsOnly,
    })
}

fn verify_font(face: FontFace, file: &FontFile, bytes: &[u8]) -> Result<(), FontCatalogError> {
    if bytes.len() as u64 != file.length || ContentHash::of(bytes).as_str() != file.sha256 {
        return Err(FontCatalogError::FontIntegrity(face));
    }
    Ok(())
}

fn verify_license(license: FontLicense, bytes: &[u8]) -> Result<(), FontCatalogError> {
    if ContentHash::of(bytes).as_str() != license.sha256() {
        return Err(FontCatalogError::LicenseIntegrity(license));
    }
    Ok(())
}

/// Unchanged upstream bytes of one face in one container.
pub fn face_bytes(face: FontFace, format: FontFormat) -> &'static [u8] {
    use FontFormat::*;
    match (face, format) {
        (FontFace::PretendardVariable, Truetype) => {
            include_bytes!("../../assets/fonts/pretendard/PretendardVariable.ttf")
        }
        (FontFace::PretendardVariable, Woff2) => {
            include_bytes!("../../assets/fonts/pretendard/PretendardVariable.woff2")
        }
        (FontFace::PretendardJpRegular, Truetype) => {
            include_bytes!("../../assets/fonts/pretendard/PretendardJP-Regular.ttf")
        }
        (FontFace::PretendardJpRegular, Woff2) => {
            include_bytes!("../../assets/fonts/pretendard/PretendardJP-Regular.woff2")
        }
        (FontFace::SourceSerif4Variable, Truetype) => {
            include_bytes!("../../assets/fonts/source-serif-4/SourceSerif4Variable-Roman.ttf")
        }
        (FontFace::SourceSerif4Variable, Woff2) => {
            include_bytes!("../../assets/fonts/source-serif-4/SourceSerif4Variable-Roman.woff2")
        }
    }
}

/// Complete upstream notice text for one license.
pub fn license_text(license: FontLicense) -> &'static str {
    match license {
        FontLicense::Pretendard => include_str!("../../assets/fonts/pretendard/LICENSE.txt"),
        FontLicense::PretendardJp => include_str!("../../assets/fonts/pretendard/LICENSE-JP.txt"),
        FontLicense::SourceSerif4 => include_str!("../../assets/fonts/source-serif-4/LICENSE.md"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_length_font_tampering_is_rejected() {
        let face = FontFace::PretendardVariable;
        let format = FontFormat::Woff2;
        let mut bytes = face_bytes(face, format).to_vec();
        bytes[0] ^= 1;
        assert_eq!(
            verify_font(face, &face.file(format), &bytes),
            Err(FontCatalogError::FontIntegrity(face)),
        );
    }

    #[test]
    fn incomplete_or_changed_license_is_rejected() {
        let license = FontLicense::PretendardJp;
        let bytes = license_text(license).as_bytes();
        assert_eq!(
            verify_license(license, &bytes[..bytes.len() - 1]),
            Err(FontCatalogError::LicenseIntegrity(license)),
        );
    }
}

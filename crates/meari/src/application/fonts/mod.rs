//! Explicitly selected retained font assets over the existing verified resource API.
mod han_serif_slices;

use super::assets::VerifiedAssets;
use crate::domain::{
    assets::{AssetDescriptor, AssetManifest},
    build::{BuildPath, ContentHash, InputRole},
    design::fonts::{
        FontFace, FontFile, FontFormat, FontLicense, FontRequirement, FontSupport, LICENSES,
        SANS_FONT_FAMILY, SERIF_FONT_FAMILY, UI_FACES, UI_FONT_FAMILY, WebSlice,
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
    /// Web resources: upstream WOFF2 of the same faces, or renamed unicode-range slices.
    pub web_assets: VerifiedAssets,
    /// Web `@font-face` rules: WOFF2 only, `font-display:swap`.
    pub css: String,
    /// Native `@font-face` rules over the same faces' upstream TTF/OTF.
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
        let native = face.native_file();
        let bytes = face_bytes(face);
        verify_font(face, &native, bytes)?;
        let mut deliveries = vec![(1, face.family(), native, None, bytes)];
        for web in web_files(face) {
            verify_font(face, &web.file, web.bytes)?;
            deliveries.push((0, face.web_family(), web.file, web.unicode_range, web.bytes));
        }
        for (slot, family, file, range, bytes) in deliveries {
            let hash = ContentHash::of(bytes);
            let descriptor = AssetDescriptor {
                role: InputRole::Font,
                content: hash.clone(),
                length: file.length,
                media_type: file.format.media_type().into(),
                license: Some(format!("OFL-1.1; {}", face.license().path())),
            };
            let output = descriptor.output_path().map_err(FontCatalogError::Assets)?;
            let range = range
                .map(|range| format!("unicode-range:{range};"))
                .unwrap_or_default();
            let rule = format!(
                "@font-face{{font-family:\"{family}\";src:url(\"/{}\") format(\"{}\");font-style:normal;font-weight:{};{range}font-display:swap;}}\n",
                output.as_str(),
                file.format.css(),
                face.weight(),
            );
            if slot == 0 {
                css.push_str(&rule)
            } else {
                native_css.push_str(&rule)
            }
            let source =
                BuildPath::try_from(file.path.to_owned()).map_err(FontCatalogError::Assets)?;
            files[slot].insert(source, descriptor);
            blobs[slot].insert(hash, bytes.to_vec());
        }
    }
    let root = format!(
        ":root{{--meari-font-ui:{UI_FONT_FAMILY};--font-sans:{SANS_FONT_FAMILY};--default-font-family:{SANS_FONT_FAMILY};--font-serif:{SERIF_FONT_FAMILY};}}\n"
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

/// One Web delivery file: the upstream WOFF2, or a renamed unicode-range slice.
#[derive(Clone, Copy, Debug)]
pub struct WebFile {
    pub file: FontFile,
    pub unicode_range: Option<&'static str>,
    pub bytes: &'static [u8],
}

/// Web WOFF2 files of one face, in `@font-face` order.
pub fn web_files(face: FontFace) -> Vec<WebFile> {
    use FontFace::*;
    let whole = |bytes: &'static [u8]| {
        let file = face.web_woff2().expect("whole-file Web face");
        vec![WebFile {
            file,
            unicode_range: None,
            bytes,
        }]
    };
    match face {
        PretendardVariable => whole(include_bytes!(
            "../../../assets/fonts/pretendard/PretendardVariable.woff2"
        )),
        PretendardJpRegular => whole(include_bytes!(
            "../../../assets/fonts/pretendard/PretendardJP-Regular.woff2"
        )),
        PretendardJpBold => whole(include_bytes!(
            "../../../assets/fonts/pretendard/PretendardJP-Bold.woff2"
        )),
        SourceSerif4Variable => whole(include_bytes!(
            "../../../assets/fonts/source-serif-4/SourceSerif4Variable-Roman.woff2"
        )),
        SourceHanSerifKrRegular => han_serif_slices::SLICES
            .iter()
            .zip(han_serif_slices::BYTES)
            .map(|(slice, bytes)| WebFile {
                file: FontFile {
                    path: slice.path,
                    sha256: slice.sha256,
                    length: slice.length,
                    format: FontFormat::Woff2,
                },
                unicode_range: Some(slice.unicode_range),
                bytes,
            })
            .collect(),
    }
}

/// Renamed unicode-range slices of Source Han Serif KR (`Meari Serif KR`).
pub fn han_serif_kr_slices() -> &'static [WebSlice] {
    &han_serif_slices::SLICES
}

/// Unchanged upstream bytes of the native file of one face.
pub fn face_bytes(face: FontFace) -> &'static [u8] {
    use FontFace::*;
    match face {
        PretendardVariable => {
            include_bytes!("../../../assets/fonts/pretendard/PretendardVariable.ttf")
        }
        PretendardJpRegular => {
            include_bytes!("../../../assets/fonts/pretendard/PretendardJP-Regular.ttf")
        }
        PretendardJpBold => {
            include_bytes!("../../../assets/fonts/pretendard/PretendardJP-Bold.ttf")
        }
        SourceSerif4Variable => {
            include_bytes!("../../../assets/fonts/source-serif-4/SourceSerif4Variable-Roman.ttf")
        }
        SourceHanSerifKrRegular => {
            include_bytes!("../../../assets/fonts/source-han-serif-kr/SourceHanSerifKR-Regular.otf")
        }
    }
}

/// Complete upstream notice text for one license.
pub fn license_text(license: FontLicense) -> &'static str {
    match license {
        FontLicense::Pretendard => include_str!("../../../assets/fonts/pretendard/LICENSE.txt"),
        FontLicense::PretendardJp => {
            include_str!("../../../assets/fonts/pretendard/LICENSE-JP.txt")
        }
        FontLicense::SourceSerif4 => {
            include_str!("../../../assets/fonts/source-serif-4/LICENSE.md")
        }
        FontLicense::SourceHanSerifKr => {
            include_str!("../../../assets/fonts/source-han-serif-kr/LICENSE.txt")
        }
        FontLicense::MeariSerifKr => {
            include_str!("../../../assets/fonts/meari-serif-kr/LICENSE.txt")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_length_font_tampering_is_rejected() {
        let face = FontFace::PretendardVariable;
        let mut bytes = face_bytes(face).to_vec();
        bytes[0] ^= 1;
        assert_eq!(
            verify_font(face, &face.native_file(), &bytes),
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

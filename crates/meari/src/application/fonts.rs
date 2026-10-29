//! Explicitly selected retained font assets over the existing verified resource API.
use super::assets::VerifiedAssets;
use crate::domain::{
    assets::{AssetDescriptor, AssetManifest},
    build::{BuildPath, ContentHash, InputRole},
    design::fonts::{
        FontFace, FontLicense, FontRequirement, FontSupport, UI_FACES, UI_FONT_FAMILY,
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
    pub assets: VerifiedAssets,
    pub css: String,
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
    for license in [FontLicense::Pretendard, FontLicense::PretendardJp] {
        verify_license(license, license_text(license).as_bytes())?;
    }
    let mut files = BTreeMap::new();
    let mut blobs = BTreeMap::new();
    let mut css = String::new();
    for face in UI_FACES {
        let bytes = font_bytes(face);
        verify_font(face, bytes)?;
        let hash = ContentHash::of(bytes);
        let descriptor = AssetDescriptor {
            role: InputRole::Font,
            content: hash.clone(),
            length: face.length(),
            media_type: "font/ttf".into(),
            license: Some(format!("OFL-1.1; {}", face.license().path())),
        };
        let output = descriptor.output_path().map_err(FontCatalogError::Assets)?;
        css.push_str(&format!(
            "@font-face{{font-family:\"{}\";src:url(\"/{}\") format(\"truetype\");font-style:normal;font-weight:{};font-display:swap;}}\n",
            face.family(), output.as_str(), face.weight(),
        ));
        let source =
            BuildPath::try_from(face.source_path().to_owned()).map_err(FontCatalogError::Assets)?;
        files.insert(source, descriptor);
        blobs.insert(hash, bytes.to_vec());
    }
    css.push_str(&format!(":root{{--meari-font-ui:{UI_FONT_FAMILY};}}\n"));
    let assets = VerifiedAssets::new(
        AssetManifest {
            schema_version: 1,
            files,
        },
        blobs,
    )
    .map_err(FontCatalogError::Assets)?;
    Ok(PreparedFontCatalog {
        assets,
        css,
        support: FontSupport::PinnedAssetsOnly,
    })
}

fn verify_font(face: FontFace, bytes: &[u8]) -> Result<(), FontCatalogError> {
    if bytes.len() as u64 != face.length() || ContentHash::of(bytes).as_str() != face.sha256() {
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

fn font_bytes(face: FontFace) -> &'static [u8] {
    match face {
        FontFace::PretendardRegular => {
            include_bytes!("../../assets/fonts/pretendard/Pretendard-Regular.ttf")
        }
        FontFace::PretendardSemiBold => {
            include_bytes!("../../assets/fonts/pretendard/Pretendard-SemiBold.ttf")
        }
        FontFace::PretendardJpRegular => {
            include_bytes!("../../assets/fonts/pretendard/PretendardJP-Regular.ttf")
        }
    }
}

fn license_text(license: FontLicense) -> &'static str {
    match license {
        FontLicense::Pretendard => include_str!("../../assets/fonts/pretendard/LICENSE.txt"),
        FontLicense::PretendardJp => include_str!("../../assets/fonts/pretendard/LICENSE-JP.txt"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_length_font_tampering_is_rejected() {
        let face = FontFace::PretendardRegular;
        let mut bytes = font_bytes(face).to_vec();
        bytes[0] ^= 1;
        assert_eq!(
            verify_font(face, &bytes),
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

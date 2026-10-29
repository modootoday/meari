//! Declared local resources. File access, CSS parsing and decoding belong to adapters.
use super::build::{BuildPath, ContentHash, InputRole};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_ASSET_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetDescriptor {
    pub role: InputRole,
    pub content: ContentHash,
    pub length: u64,
    pub media_type: String,
    pub license: Option<String>,
}
impl AssetDescriptor {
    fn extension(&self) -> Result<&'static str, String> {
        match (&self.role, self.media_type.as_str()) {
            (InputRole::Icon | InputRole::Image, "image/svg+xml") => Ok("svg"),
            (InputRole::Image, "image/png") => Ok("png"),
            (InputRole::Image, "image/jpeg") => Ok("jpg"),
            (InputRole::Image, "image/webp") => Ok("webp"),
            (InputRole::Image, "image/gif") => Ok("gif"),
            (InputRole::Font, "font/ttf") => Ok("ttf"),
            (InputRole::Font, "font/otf") => Ok("otf"),
            (InputRole::Font, "font/woff") => Ok("woff"),
            (InputRole::Font, "font/woff2") => Ok("woff2"),
            _ => Err("asset role or media type is unsupported".into()),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.extension()?;
        if self.length == 0 || self.length > MAX_ASSET_BYTES {
            return Err("asset byte length exceeds its budget".into());
        }
        if self.license.as_ref().is_some_and(|license| {
            license.trim().is_empty()
                || license.len() > 8192
                || license.chars().any(char::is_control)
        }) {
            return Err("asset license metadata is invalid".into());
        }
        Ok(())
    }
    /// A content address, independent of the original source file's display name.
    pub fn output_path(&self) -> Result<BuildPath, String> {
        self.validate()?;
        format!("assets/{}.{}", self.content.as_str(), self.extension()?).try_into()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetManifest {
    pub schema_version: u32,
    pub files: BTreeMap<BuildPath, AssetDescriptor>,
}
impl AssetManifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 || self.files.len() > 16384 {
            return Err("unsupported asset manifest or inventory budget".into());
        }
        let mut paths = BTreeSet::new();
        let mut content = BTreeMap::new();
        let mut total = 0u64;
        for (path, descriptor) in &self.files {
            descriptor.validate()?;
            if !paths.insert(path.as_str().to_ascii_lowercase()) {
                return Err("asset source paths collide across targets".into());
            }
            if let Some((length, media_type)) = content.insert(
                descriptor.content.clone(),
                (descriptor.length, descriptor.media_type.as_str()),
            ) {
                if length != descriptor.length || media_type != descriptor.media_type {
                    return Err("same asset content has conflicting byte or media metadata".into());
                }
            } else {
                total = total
                    .checked_add(descriptor.length)
                    .ok_or("asset byte budget overflow")?;
                if total > MAX_ASSET_TOTAL_BYTES {
                    return Err("asset inventory exceeds byte budget".into());
                }
            }
        }
        Ok(())
    }
    pub fn fingerprint(&self) -> Result<ContentHash, String> {
        self.validate()?;
        Ok(ContentHash::of(
            &serde_json::to_vec(self).map_err(|error| error.to_string())?,
        ))
    }
}

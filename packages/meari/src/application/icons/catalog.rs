//! Content verification over a separately reviewed, immutable upstream lock.
//! This does not attest publisher identity or render untrusted SVG as HTML.
use crate::domain::{
    build::ContentHash,
    design::icons::catalog::{IconRole, LUCIDE},
};
use std::collections::BTreeMap;

pub struct IconLock {
    pub role: IconRole,
    pub source_sha256: ContentHash,
}
pub struct CatalogLock {
    pub version: String,
    pub revision: String,
    pub entries: Vec<IconLock>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogError {
    PinMismatch,
    LicenseMismatch,
    EmptyInventory,
    DuplicateRole,
    MissingAsset(IconRole),
    HashMismatch(IconRole),
    OversizedAsset(IconRole),
    UnlistedAsset(IconRole),
    NotImported(IconRole),
}
pub struct ContentVerifiedIcon {
    source_sha256: ContentHash,
    bytes: Vec<u8>,
}
impl ContentVerifiedIcon {
    pub fn source_sha256(&self) -> &ContentHash {
        &self.source_sha256
    }
    /// Bytes for the existing checked asset adapter, never interpolated HTML.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn cache_revision(&self) -> String {
        format!(
            "lucide@{}:{}:{}",
            LUCIDE.version,
            LUCIDE.revision,
            self.source_sha256.as_str()
        )
    }
}
pub struct ContentVerifiedCatalog {
    icons: BTreeMap<IconRole, ContentVerifiedIcon>,
    license: Vec<u8>,
}
impl ContentVerifiedCatalog {
    /// Upstream membership/provenance must be reviewed before accepting the lock.
    pub fn verify(
        lock: CatalogLock,
        mut assets: BTreeMap<IconRole, Vec<u8>>,
        license: Vec<u8>,
    ) -> Result<Self, CatalogError> {
        if lock.version != LUCIDE.version || lock.revision != LUCIDE.revision {
            return Err(CatalogError::PinMismatch);
        }
        if ContentHash::of(&license).as_str() != LUCIDE.license_sha256 {
            return Err(CatalogError::LicenseMismatch);
        }
        if lock.entries.is_empty() {
            return Err(CatalogError::EmptyInventory);
        }
        let mut icons = BTreeMap::new();
        for entry in lock.entries {
            if icons.contains_key(&entry.role) {
                return Err(CatalogError::DuplicateRole);
            }
            let bytes = assets
                .remove(&entry.role)
                .ok_or(CatalogError::MissingAsset(entry.role))?;
            if bytes.is_empty() || bytes.len() > 65536 {
                return Err(CatalogError::OversizedAsset(entry.role));
            }
            if ContentHash::of(&bytes) != entry.source_sha256 {
                return Err(CatalogError::HashMismatch(entry.role));
            }
            icons.insert(
                entry.role,
                ContentVerifiedIcon {
                    source_sha256: entry.source_sha256,
                    bytes,
                },
            );
        }
        if let Some(role) = assets.keys().next() {
            return Err(CatalogError::UnlistedAsset(*role));
        }
        Ok(Self { icons, license })
    }
    pub fn resolve(&self, role: IconRole) -> Result<&ContentVerifiedIcon, CatalogError> {
        self.icons.get(&role).ok_or(CatalogError::NotImported(role))
    }
    pub fn license(&self) -> &[u8] {
        &self.license
    }
}

#[cfg(test)]
mod tests;

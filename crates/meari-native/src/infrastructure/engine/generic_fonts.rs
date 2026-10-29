//! Generic CSS families (`sans-serif`, `serif`) for native documents.
//!
//! Canonical profile (default): the bundled OFL faces come first, Pretendard Variable /
//! Pretendard JP for `sans-serif` and Source Serif 4 then an installed Source Han Serif KR /
//! Noto Serif KR for `serif`, then the Chromium Linux defaults below, then the OS host fallbacks chosen by the
//! `meari-platform` adapter.
//!
//! Chrome comparison profile (`meari-visual` only): Chromium does not ask fontconfig for
//! generics; it uses per-platform preferences (Linux: Arial / Times New Roman) which
//! fontconfig substitutes with metric-compatible host faces (Arimo / Liberation, Tinos). Those
//! faces are never bundled; this profile exists only so a pixel comparison against an
//! unmodified Chrome picks the same host faces.
use meari_platform::{OsFamily, PlatformHost};
use parley::fontique::{Blob, Collection, CollectionOptions, FamilyId, GenericFamily, SourceCache};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Chromium Linux defaults followed by fontconfig's metric-compatible substitutes.
const CHROMIUM: &[(GenericFamily, &[&str])] = &[
    (
        GenericFamily::SansSerif,
        &["Arial", "Arimo", "Liberation Sans"],
    ),
    (
        GenericFamily::Serif,
        &["Times New Roman", "Tinos", "Liberation Serif"],
    ),
];

static CHROME_COMPARISON: AtomicBool = AtomicBool::new(false);

/// Switches every later document of this process to the Chrome comparison profile.
pub fn use_chrome_comparison_profile() {
    CHROME_COMPARISON.store(true, Ordering::SeqCst);
}

/// Font context for a document: system fonts, bundled faces, blitz's bullet font, generics.
pub fn context() -> parley::FontContext {
    build(!CHROME_COMPARISON.load(Ordering::SeqCst))
}

fn build(canonical: bool) -> parley::FontContext {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: true,
    });
    collection.register_fonts(Blob::new(Arc::new(blitz_dom::BULLET_FONT) as _), None);
    register_bundled(&mut collection);
    let host = meari_platform::current();
    for (generic, chromium) in CHROMIUM {
        let mut names: Vec<&str> = Vec::new();
        if canonical {
            names.extend(canonical_names(*generic));
        }
        if canonical || host.os_family() == OsFamily::Unix {
            names.extend(chromium.iter().copied());
        }
        if canonical {
            names.extend(host_names(&host, *generic));
        }
        prefer(&mut collection, *generic, &names);
    }
    parley::FontContext {
        source_cache: SourceCache::new_shared(),
        collection,
    }
}

/// OS host fallbacks from the platform adapter, after the canonical and Chromium names.
fn host_names(host: &impl PlatformHost, generic: GenericFamily) -> Vec<&'static str> {
    let fonts = host.font_fallbacks();
    match generic {
        GenericFamily::SansSerif => fonts.sans.to_vec(),
        GenericFamily::Serif => fonts.serif.to_vec(),
        _ => Vec::new(),
    }
}

#[cfg(feature = "bundled-fonts")]
fn canonical_names(generic: GenericFamily) -> Vec<&'static str> {
    use meari::domain::design::fonts::{SANS_FAMILIES, SERIF_FAMILIES};
    match generic {
        GenericFamily::SansSerif => SANS_FAMILIES.to_vec(),
        GenericFamily::Serif => SERIF_FAMILIES.to_vec(),
        _ => Vec::new(),
    }
}

#[cfg(not(feature = "bundled-fonts"))]
fn canonical_names(_: GenericFamily) -> Vec<&'static str> {
    Vec::new()
}

#[cfg(feature = "bundled-fonts")]
fn register_bundled(collection: &mut Collection) {
    use meari::{application::fonts::face_bytes, domain::design::fonts::UI_FACES};
    for face in UI_FACES {
        let bytes: &'static [u8] = face_bytes(face);
        collection.register_fonts(Blob::new(Arc::new(bytes) as _), None);
    }
}

#[cfg(not(feature = "bundled-fonts"))]
fn register_bundled(_: &mut Collection) {}

/// Puts the installed names, in order, in front of the generic family; others keep order.
fn prefer(collection: &mut Collection, generic: GenericFamily, names: &[&str]) {
    let mut first: Vec<FamilyId> = Vec::new();
    for name in names {
        if let Some(id) = collection.family_id(name)
            && !first.contains(&id)
        {
            first.push(id);
        }
    }
    if first.is_empty() {
        return;
    }
    let rest = collection
        .generic_families(generic)
        .filter(|id| !first.contains(id))
        .collect::<Vec<_>>();
    collection.set_generic_families(generic, first.into_iter().chain(rest));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "bundled-fonts")]
    #[test]
    fn canonical_generics_resolve_to_bundled_faces_first() {
        let mut context = build(true);
        let collection = &mut context.collection;
        let pretendard = collection.family_id("Pretendard Variable").unwrap();
        let jp = collection.family_id("Pretendard JP").unwrap();
        let serif = collection.family_id("Source Serif 4 Variable").unwrap();
        let sans = collection
            .generic_families(GenericFamily::SansSerif)
            .collect::<Vec<_>>();
        assert_eq!(&sans[..2], &[pretendard, jp]);
        let serifs = collection
            .generic_families(GenericFamily::Serif)
            .collect::<Vec<_>>();
        assert_eq!(serifs[0], serif);
        // The Arial-family host fallback, when installed, follows the canonical faces.
        if let Some(arial) = ["Arial", "Arimo", "Liberation Sans"]
            .iter()
            .find_map(|name| collection.family_id(name))
        {
            assert_eq!(sans[2], arial);
        }
    }

    #[test]
    fn chrome_comparison_profile_prefers_the_chromium_face_when_installed() {
        let mut context = build(false);
        let collection = &mut context.collection;
        let Some(expected) = ["Arial", "Arimo", "Liberation Sans"]
            .iter()
            .find_map(|name| collection.family_id(name))
        else {
            return; // no metric-compatible face installed: generics stay as fontconfig says
        };
        if cfg!(target_os = "linux") {
            assert_eq!(
                collection.generic_families(GenericFamily::SansSerif).next(),
                Some(expected)
            );
        }
    }
}

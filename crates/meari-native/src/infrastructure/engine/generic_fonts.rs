//! Generic CSS families (`sans-serif`, `serif`, `monospace`) for native documents.
//!
//! Both profiles register the pinned Noto faces from the verified font cache
//! (`infrastructure::fonts`, never compiled in) and put them first: Noto Sans (+ KR, JP,
//! Arabic, Hebrew, Devanagari, Thai) for `sans-serif`, Noto Serif (+ KR, JP, Naskh Arabic, ...)
//! for `serif`, Noto Sans Mono for `monospace`.
//!
//! Canonical profile (default): the OS host fallbacks of the `meari-platform` adapter follow the
//! Noto faces, so an app whose cache was never fetched still renders (a single stderr line names
//! `meari fonts fetch`).
//! Chrome comparison profile (`meari-visual` only): no host names at all; the harness rewrites
//! the same generics to the same Noto families on the Chrome side, so no system face is used.
use meari::domain::design::fonts::{
    MANIFEST, MONO_FAMILIES, SANS_FAMILIES, SERIF_FAMILIES, UI_SCRIPTS,
};
use meari_platform::PlatformHost;
use parley::fontique::{Blob, Collection, CollectionOptions, FamilyId, GenericFamily, SourceCache};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

const GENERICS: [GenericFamily; 3] = [
    GenericFamily::SansSerif,
    GenericFamily::Serif,
    GenericFamily::Monospace,
];

static CHROME_COMPARISON: AtomicBool = AtomicBool::new(false);
/// Switches every later document of this process to the Chrome comparison profile.
pub fn use_chrome_comparison_profile() {
    CHROME_COMPARISON.store(true, Ordering::SeqCst);
}

/// Font context for a document: system fonts, bundled faces, blitz's bullet font, generics.
pub fn context() -> parley::FontContext {
    let fonts = build(!CHROME_COMPARISON.load(Ordering::SeqCst));
    blitz_paint::set_placeholder_fonts(fonts.clone());
    fonts
}

fn build(canonical: bool) -> parley::FontContext {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: true,
    });
    collection.register_fonts(Blob::new(Arc::new(blitz_dom::BULLET_FONT) as _), None);
    for bytes in cached_faces() {
        collection.register_fonts(Blob::new(Arc::new(*bytes) as _), None);
    }
    let host = meari_platform::current();
    for generic in GENERICS {
        let mut names = noto_names(generic);
        if canonical {
            names.extend(host_names(&host, generic));
        }
        prefer(&mut collection, generic, &names);
    }
    parley::FontContext {
        source_cache: SourceCache::new_shared(),
        collection,
    }
}

fn noto_names(generic: GenericFamily) -> Vec<&'static str> {
    match generic {
        GenericFamily::SansSerif => SANS_FAMILIES.to_vec(),
        GenericFamily::Serif => SERIF_FAMILIES.to_vec(),
        GenericFamily::Monospace => MONO_FAMILIES.to_vec(),
        _ => Vec::new(),
    }
}

/// OS host fallbacks from the platform adapter, after the Noto names (canonical profile only).
fn host_names(host: &impl PlatformHost, generic: GenericFamily) -> Vec<&'static str> {
    let fonts = host.font_fallbacks();
    match generic {
        GenericFamily::SansSerif => fonts.sans.to_vec(),
        GenericFamily::Serif => fonts.serif.to_vec(),
        _ => Vec::new(),
    }
}

/// Verified files of the default font cache, loaded once per process. Every cached pin is
/// registered; a missing or damaged canonical (Latin/Hangul/Japanese) file prints one line
/// naming `meari fonts fetch` and the document falls back to host fonts. Never downloads.
fn cached_faces() -> &'static [&'static [u8]] {
    static FACES: OnceLock<Vec<&'static [u8]>> = OnceLock::new();
    FACES.get_or_init(|| {
        let cache = crate::infrastructure::fonts::FontCache::default_location();
        let mut faces = Vec::new();
        let mut problem = None;
        for pin in MANIFEST {
            match cache.load(pin) {
                Ok(bytes) => faces.push(&*Box::leak(bytes.into_boxed_slice())),
                Err(e) if UI_SCRIPTS.contains(&pin.script) => {
                    problem.get_or_insert(e);
                }
                Err(_) => {}
            }
        }
        if let Some(e) = problem {
            eprintln!(
                "meari: canonical Noto fonts unavailable in {} ({e}); using host fonts until `meari fonts fetch` is run",
                cache.root().display()
            );
        }
        faces
    })
}

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

    #[test]
    fn both_profiles_resolve_generics_to_noto_first() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/fonts");
        crate::infrastructure::fonts::FontCache::at(&root)
            .ensure(&["latin", "hangul", "kana"])
            .unwrap();
        for canonical in [true, false] {
            let mut context = build(canonical);
            let collection = &mut context.collection;
            for (generic, first) in [
                (
                    GenericFamily::SansSerif,
                    ["Noto Sans", "Noto Sans KR", "Noto Sans JP"],
                ),
                (
                    GenericFamily::Serif,
                    ["Noto Serif", "Noto Serif KR", "Noto Serif JP"],
                ),
                (
                    GenericFamily::Monospace,
                    ["Noto Sans Mono", "Noto Sans KR", "Noto Sans JP"],
                ),
            ] {
                let ids = first.map(|name| collection.family_id(name).unwrap());
                let resolved = collection.generic_families(generic).collect::<Vec<_>>();
                assert_eq!(&resolved[..3], &ids, "{generic:?}");
            }
        }
    }
}

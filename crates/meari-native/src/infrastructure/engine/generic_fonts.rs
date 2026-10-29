//! Generic CSS families resolved like Chromium's default font preferences.
//!
//! Chromium does not ask fontconfig for `sans-serif`/`serif`; it uses its own per-platform
//! preferences (Linux: Arial / Times New Roman), which fontconfig then substitutes with
//! metric-compatible faces (Arimo / Liberation, Tinos). Native must pick the same faces or text
//! widths drift from the Web path.
use parley::fontique::{Blob, Collection, CollectionOptions, GenericFamily, SourceCache};
use std::sync::Arc;

/// Chromium Linux defaults followed by fontconfig's metric-compatible substitutes.
const PREFERENCES: &[(GenericFamily, &[&str])] = &[
    (
        GenericFamily::SansSerif,
        &["Arial", "Arimo", "Liberation Sans"],
    ),
    (
        GenericFamily::Serif,
        &["Times New Roman", "Tinos", "Liberation Serif"],
    ),
];

/// Font context for a document: system fonts, blitz's list-bullet font, Chromium generics.
pub fn context() -> parley::FontContext {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: true,
    });
    collection.register_fonts(Blob::new(Arc::new(blitz_dom::BULLET_FONT) as _), None);
    if cfg!(target_os = "linux") {
        apply(&mut collection);
    }
    parley::FontContext {
        source_cache: SourceCache::new_shared(),
        collection,
    }
}

/// Puts the first installed preference in front of each generic family; unchanged otherwise.
fn apply(collection: &mut Collection) {
    for (generic, names) in PREFERENCES {
        let Some(first) = names.iter().find_map(|name| collection.family_id(name)) else {
            continue;
        };
        let rest = collection
            .generic_families(*generic)
            .filter(|id| *id != first)
            .collect::<Vec<_>>();
        collection.set_generic_families(*generic, std::iter::once(first).chain(rest));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sans_serif_prefers_the_chromium_face_when_installed() {
        let mut context = context();
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

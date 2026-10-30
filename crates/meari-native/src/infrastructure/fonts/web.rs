//! WOFF2 `unicode-range` slices generated from the verified cached TTF (Noto families have no
//! Reserved Font Name, so subsets keep the family name). Generated once into
//! `runtime/fonts/web/<rev>-<generator>/` with an index of sha256 values checked on every load.
use super::cache::{FontCache, hex_sha256};
use meari::domain::design::fonts::{Pin, REVISION};
use std::path::PathBuf;

/// Bumped whenever the slicing or encoding below changes, so stale outputs are never reused.
const GENERATOR: &str = "s5";
/// Code points per slice after the first (Latin and friends below U+1100 always form slice 0).
const SLICE_CODEPOINTS: usize = 2000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebSlice {
    /// File name under the web cache directory, e.g. `NotoSansKR.3.woff2`.
    pub name: String,
    /// CSS `unicode-range` value.
    pub unicode_range: String,
    pub path: PathBuf,
    pub sha256: String,
}

fn web_dir(cache: &FontCache) -> PathBuf {
    cache
        .root()
        .join("web")
        .join(format!("{}-{GENERATOR}", &REVISION[..12]))
}

fn stem(pin: &Pin) -> String {
    pin.file
        .split('[')
        .next()
        .unwrap_or(pin.file)
        .trim_end_matches(".ttf")
        .to_string()
}

/// Slices of one pinned face, generating them from the cache if absent or invalid.
pub fn web_slices(cache: &FontCache, pin: &Pin) -> Result<Vec<WebSlice>, String> {
    let dir = web_dir(cache);
    let index = dir.join(format!("{}.index", stem(pin)));
    if let Ok(slices) = read_index(&dir, &index) {
        return Ok(slices);
    }
    let source = cache.load(pin)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut lines = String::new();
    for (n, group) in groups(&source)?.iter().enumerate() {
        let subset = subset(&source, group).map_err(|e| format!("subset {}: {e}", pin.file))?;
        let woff2 = ttf2woff2::encode(&subset, ttf2woff2::BrotliQuality::default())
            .map_err(|e| format!("woff2 {}: {e:?}", pin.file))?;
        let name = format!("{}.{n}.woff2", stem(pin));
        let path = dir.join(&name);
        write_atomic(&path, &woff2)?;
        lines.push_str(&format!(
            "{name}\t{}\t{}\n",
            hex_sha256(&woff2),
            ranges(group)
        ));
    }
    write_atomic(&index, lines.as_bytes())?;
    read_index(&dir, &index)
}

/// Concurrent generators (parallel tests, two processes) never expose a partial file.
fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp{}-{n}", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

fn read_index(dir: &std::path::Path, index: &std::path::Path) -> Result<Vec<WebSlice>, String> {
    let text = std::fs::read_to_string(index).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for line in text.lines() {
        let mut parts = line.split('\t');
        let (Some(name), Some(sha256), Some(range)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err("bad index".into());
        };
        let path = dir.join(name);
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        if hex_sha256(&bytes) != sha256 {
            return Err(format!("{} changed", path.display()));
        }
        out.push(WebSlice {
            name: name.into(),
            unicode_range: range.into(),
            path,
            sha256: sha256.into(),
        });
    }
    if out.is_empty() {
        return Err("empty index".into());
    }
    Ok(out)
}

/// Subset to `unicodes` with skera (keeps variable GPOS, which fontcull-klippa 0.1.2 silently
/// dropped) and the default layout features of every script. HVAR must survive:
/// Noto's gvar carries no phantom-point advance deltas, so without HVAR every weight would get
/// the default-instance advances. fontcull-klippa 0.1.2 panicked (hvar.rs:102) on an HVAR with an implicit
/// advance map (Noto Sans KR), so [`explicit_hvar`] first writes that map out explicitly.
/// Vertical metrics (vhea/vmtx/VVAR/VORG) are dropped: klippa 0.1.2 left vhea.numberOfVMetrics
/// above the subset glyph count, which Chrome's OTS sanitizer rejects (the whole slice is then
/// skipped and Chrome falls back to a host font); the harness only lays out horizontal text.
fn subset(font: &[u8], unicodes: &[u32]) -> Result<Vec<u8>, String> {
    use read_fonts_subset::collections::IntSet;
    use skera::{DEFAULT_LAYOUT_FEATURES, Plan, SubsetFlags, subset_font};
    use skrifa_subset::{FontRef, GlyphId, Tag};
    use write_fonts::types::NameId;
    let font = explicit_hvar(font)?;
    let font = FontRef::new(&font).map_err(|e| format!("{e:?}"))?;
    let plan = Plan::new(
        &IntSet::<GlyphId>::empty(),
        &IntSet::from_iter(unicodes.iter().copied()),
        &font,
        SubsetFlags::default(),
        &IntSet::from_iter([b"vhea", b"vmtx", b"VVAR", b"VORG"].map(Tag::new)),
        &IntSet::all(),
        &IntSet::from_iter(DEFAULT_LAYOUT_FEATURES.iter().copied()),
        &IntSet::<NameId>::empty(),
        &IntSet::<u16>::empty(),
    );
    subset_font(&font, &plan).map_err(|e| format!("{e:?}"))
}

/// Same font, with an implicit HVAR advance-width mapping (glyph id = inner index, outer 0)
/// replaced by the equivalent explicit format-0 map. Fonts without HVAR, or with an explicit
/// map already, come back unchanged.
fn explicit_hvar(font: &[u8]) -> Result<Vec<u8>, String> {
    use read_fonts_subset::{FontRef, TableProvider};
    use write_fonts::{
        FontBuilder,
        from_obj::ToOwnedTable,
        tables::{
            hvar::Hvar,
            variations::{DeltaSetIndexMap, EntryFormat},
        },
    };
    let face = FontRef::new(font).map_err(|e| format!("{e:?}"))?;
    let Ok(hvar) = face.hvar() else {
        return Ok(font.to_vec());
    };
    if hvar.advance_width_mapping().is_some() {
        return Ok(font.to_vec());
    }
    let glyphs = face.maxp().map_err(|e| format!("{e:?}"))?.num_glyphs();
    let mut owned: Hvar = hvar.to_owned_table();
    // 2-byte entries, 16 inner bits, 0 outer bits: entry n = (outer 0, inner n).
    let data = (0..glyphs).flat_map(u16::to_be_bytes).collect();
    owned.advance_width_mapping = Some(DeltaSetIndexMap::format_0(
        EntryFormat::from_bits_truncate(0x1F),
        glyphs,
        data,
    ))
    .into();
    let mut builder = FontBuilder::new();
    builder.add_table(&owned).map_err(|e| format!("{e:?}"))?;
    builder.copy_missing_tables(face);
    Ok(builder.build())
}

/// Mapped code points of the face (cmap, glyph != .notdef), partitioned into slices.
pub(super) fn groups(font: &[u8]) -> Result<Vec<Vec<u32>>, String> {
    let face = ttf_parser::Face::parse(font, 0).map_err(|e| format!("{e}"))?;
    let mut cps = std::collections::BTreeSet::new();
    for table in face.tables().cmap.into_iter().flat_map(|c| c.subtables) {
        if table.is_unicode() {
            table.codepoints(|cp| {
                if table.glyph_index(cp).is_some_and(|g| g.0 != 0) {
                    cps.insert(cp);
                }
            });
        }
    }
    let (low, high): (Vec<u32>, Vec<u32>) = cps.into_iter().partition(|cp| *cp < 0x1100);
    let mut out = Vec::new();
    if !low.is_empty() {
        out.push(low);
    }
    out.extend(high.chunks(SLICE_CODEPOINTS).map(<[u32]>::to_vec));
    Ok(out)
}

/// `U+20-7E,U+A0` style range list of sorted code points.
pub(super) fn ranges(cps: &[u32]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < cps.len() {
        let start = cps[i];
        let mut end = start;
        while i + 1 < cps.len() && cps[i + 1] == end + 1 {
            i += 1;
            end = cps[i];
        }
        out.push(if start == end {
            format!("U+{start:X}")
        } else {
            format!("U+{start:X}-{end:X}")
        });
        i += 1;
    }
    out.join(",")
}

#[cfg(test)]
mod tests {
    #[test]
    fn ranges_collapse_runs() {
        assert_eq!(
            super::ranges(&[0x20, 0x21, 0x22, 0x41, 0xAC00]),
            "U+20-22,U+41,U+AC00"
        );
    }

    /// The Latin slice keeps the variable GPOS (kerning with device tables): fontcull-klippa
    /// 0.1.2 silently dropped it, so Chrome laid Latin text out without kerning.
    #[test]
    fn latin_slice_keeps_variable_gpos() {
        use read_fonts_subset::{FontRef, types::Tag};
        let cache =
            super::FontCache::at(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/fonts"));
        let pins = cache.ensure(&["latin"]).unwrap();
        assert!(!pins.is_empty());
        for (pin, _) in pins {
            let source = cache.load(&pin).unwrap();
            let group = &super::groups(&source).unwrap()[0];
            let out = super::subset(&source, group).unwrap();
            let gpos = Tag::new(b"GPOS");
            assert!(FontRef::new(&source).unwrap().table_data(gpos).is_some());
            assert!(
                FontRef::new(&out).unwrap().table_data(gpos).is_some(),
                "{}",
                pin.file
            );
        }
    }
}

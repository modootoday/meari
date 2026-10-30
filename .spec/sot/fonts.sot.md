---
id: sot:meari/fonts
title: Canonical fonts, licensing and delivery (pinned Noto downloads, Native TTF / Web WOFF2)
status: active
verify: cargo test -p meari --lib fonts; cargo test -p meari-native --test font_catalog; cargo test -p meari-native --lib fonts; cargo xtask visual-test
created: 2026-10-30
updated: 2026-10-30
rel:
  governs: [crates/meari/src/domain/design/fonts.rs, crates/meari-native/src/infrastructure/fonts, crates/meari-native, crates/meari-visual]
---

# Canonical fonts, licensing and delivery

## Rule (decision 2026-10-30, owner)

- **Noto everywhere, downloaded, never committed**: every canonical face is a Noto family
  (SIL OFL-1.1) from `github.com/google/fonts` at revision
  `6157dfaa640ca5738b4d1a7070fe389c3036d7fc`. No font file is committed and no font byte is
  `include_bytes!`'d into any binary (the former Pretendard / Pretendard JP / Source Serif 4 /
  Source Han Serif KR / Meari Serif KR files, the `bundled-fonts` feature and
  `scripts/fonts` were deleted).
- **Manifest**: `crates/meari/src/domain/design/fonts.rs` (pure data, single source): `REVISION`,
  `MANIFEST` (17 pins: URL path, length, sha256, family, style, weight range, script, and the
  directory's `OFL.txt` length/sha256), `select(scripts)`, `UI_SCRIPTS = [latin, hangul, kana]`.
- **Stacks**: sans `"Noto Sans", "Noto Sans KR", "Noto Sans JP", "Noto Sans Arabic",
  "Noto Sans Hebrew", "Noto Sans Devanagari", "Noto Sans Thai", sans-serif`; serif `"Noto Serif",
  "Noto Serif KR", "Noto Serif JP", "Noto Naskh Arabic", "Noto Serif Hebrew",
  "Noto Serif Devanagari", "Noto Serif Thai", serif`; mono `"Noto Sans Mono", "Noto Sans KR",
  "Noto Sans JP", monospace`. Tailwind `@theme` declares `--font-sans/--font-serif/--font-mono`
  with these stacks. Test `every_stack_family_is_pinned`.
- **Generics**: native `generic_fonts` puts the cached Noto faces first for `sans-serif`,
  `serif`, `monospace` in both profiles; the canonical profile then appends the per-OS host
  fallbacks (`meari-platform` `FONT_FALLBACKS`), the comparison profile appends none.
- **Tolerance**: a fixture's tolerance is never raised to make a font change pass.

## Runtime cache and missing-font behaviour

- Location: `$MEARI_FONT_CACHE`, else the nearest ancestor `runtime/fonts`, else
  `./runtime/fonts` (`FontCache::default_location`). Layout: `noto/<rev12>/<file>` plus
  `noto/<rev12>/<dir>-OFL.txt`; Web slices in `web/<rev12>-s4/`.
- **Explicit download only**: `meari fonts fetch [all|latin,hangul,kana,arabic,hebrew,devanagari,thai]`
  (or `cargo xtask fonts-fetch …`) downloads missing files with `curl -fsSL`, verifies length and
  sha256 and writes atomically. A valid cached file is used offline.
- **The app never downloads.** On first run without a cache, native prints one stderr line
  `meari: canonical Noto fonts unavailable in <dir> (…); using host fonts until `meari fonts fetch` is run`
  and renders with the host fallbacks (degraded, not a crash). A corrupted file (length/sha256
  mismatch) is an error naming expected/actual values and the fetch command, never skipped
  (tests `missing_file_names_the_fetch_command`, `checksum_mismatch_is_a_clear_error`,
  `corrupted_cache_file_is_an_error_not_a_skip`). Auto-download on first run was rejected: no
  hidden network access from a UI binary.
- Tests and `cargo xtask visual-test` call `ensure` (download if missing) for what they need.

## Web delivery (WOFF2 subsets generated from the cache)

- `meari_native::infrastructure::fonts::web_slices(cache, pin)` subsets each cached TTF with
  fontcull-klippa 0.1.2 (default layout features of every script), encodes WOFF2 with
  ttf2woff2 0.14.1 (pure Rust) and writes atomically to `web/<rev12>-s4/<Name>.<n>.woff2` plus
  `<Name>.index` (`name\tsha256\tunicode-range`). Slicing: code points < U+1100 in slice 0, the
  rest in chunks of 2,000. Union of the slices = the TTF cmap (test
  `web_slices_cover_exactly_the_native_cmap`). Full cache: 66 files, 27.9 MB.
- Subsetter workarounds (generator `s4`):
  - klippa panics (hvar.rs:102) on an HVAR with an implicit advance map (Noto Sans KR).
    `explicit_hvar` rewrites that map as an equivalent explicit format-0 map first. HVAR cannot
    be dropped: Noto's gvar has no phantom-point advance deltas, so every weight would get the
    default-instance advances (measured: 'A' 574 vs 607.5 units at wght 400).
  - vhea/vmtx/VVAR/VORG are dropped: klippa leaves `vhea.numberOfVMetrics` above the subset
    glyph count, Chrome's OTS rejects the slice and silently falls back to a host font
    (observed: tiny kana in canonical-font). Horizontal text only.
- `prepare_font_catalog(cache, req)`: `css` (WOFF2 slices, `font-display:swap`) for Web,
  `native_css` + TTF assets for native (blitz has no WOFF2 decoder), licences per family.
- Comparison harness: `font-display:block`; `style::chrome_generics` rewrites the CSS keywords
  `sans-serif`/`serif`/`monospace` to the Noto families on the Chrome side only (Chrome resolves
  generics through OS settings CSS cannot change), so `generic-font-family` no longer depends on
  Windows Arial / Linux Arimo.

## OFL / Reserved Font Name

Noto families have no Reserved Font Name, except the Noto Sans/Serif KR and JP notices, which
carry Adobe's 'Source' RFN from Source Han. Web slices are subsets + WOFF2 conversion, i.e. an
OFL *Modified Version*; their family names ("Noto Sans KR", …) contain no reserved name, keep
the copyright lines, and the unchanged `OFL.txt` sits in the cache next to them.

## Script coverage (native shaping, measured 2026-10-30 Linux)

parley 0.12 / harfrust 0.12 shape, unicode-bidi orders. Fixtures `supported/script-{arabic,
hebrew,devanagari,thai}` (24px, `dir="rtl"` for Arabic/Hebrew): all MATCH, glyph AA only, ink
bbox equal — arabic 0.505%, hebrew 0.557%, devanagari 0.518%, thai 0.625% (tolerance 0.8%).
Found: the native DOM rejects the `lang` attribute ("native DOM prop not implemented: p.lang"),
so the fixtures omit it. Extending scripts: add `Pin` rows from
`github.com/google/fonts/tree/<revision>/ofl/<dir>` plus a test case.

## Visual results (Linux, 2026-10-30)

- 52 fixtures: 51 MATCH, 1 MISMATCH. `generic-font-family` 0.704% (≤1.0%).
- **canonical-font fails: 2.951% > 1.7% (not loosened)**. Per-line ink bbox equal (±1 px),
  best shift is (0,0) on all 6 lines, signed error unbiased (53% native-darker), deltas spread
  17–255. So it is vello_cpu vs Skia glyph AA/sub-pixel placement on dense Noto CJK strokes,
  larger than with the former Pretendard faces (1.597%). Owner decision needed (shrink/split the
  fixture, or accept a new value); the limit stays 1.7% until then.

## OS anti-aliasing policy (decision 2026-10-30, owner)

- OS-specific AA differences are accepted; Linux is the gate. `PixelGate::current()`: Unix ⇒
  Strict; Windows/macOS ⇒ Advisory (pixel MISMATCH with matching ink layout ⇒
  `ADVISORY (OS rendering difference)`; LAYOUT-MISMATCH and ERROR still fail).
  `--gate strict|advisory` overrides. **Unverified**: macOS.

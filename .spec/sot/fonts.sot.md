---
id: sot:meari/fonts
title: Canonical fonts, licensing and delivery (Native TTF / Web WOFF2)
status: draft
verify: cargo test -p meari --features bundled-fonts --test font_catalog; cargo test -p meari-native --test bundled_font_catalog; cargo xtask visual-test
created: 2026-10-30
updated: 2026-10-30
rel:
  governs: [crates/meari/assets/fonts, crates/meari-native, crates/meari-visual]
---

# Canonical fonts, licensing and delivery

## Rule

- **sans-serif canonical**: Pretendard Variable (wght 45–930), then an installed static
  Pretendard, then bundled Pretendard JP (kana/kanji), then the prior Arial-family fallback.
  Stack: `"Pretendard Variable", Pretendard, "Pretendard JP", Arial, sans-serif`.
- **serif canonical**: Source Serif 4 Variable (wght 200–900, opsz 8–60), then host
  Source Han Serif KR / Noto Serif KR (not bundled), then Times New Roman, serif.
- **One licence**: every bundled font is SIL OFL-1.1, from Google/Adobe-family upstreams
  (Pretendard is built on Inter / Source Han Sans / M PLUS 1p, all OFL). Holders and Reserved Font
  Names live in `THIRD_PARTY_NOTICES.md`; full notices stay beside the fonts.
  `font_catalog::every_bundled_font_is_ofl_and_catalogued` fails on any non-OFL licence,
  any uncatalogued file, or any Arimo/Tinos/Liberation file under `assets/fonts`.
- **Native default**: with `bundled-fonts` (now a default feature of meari-native) the font
  context registers the bundled TTFs and puts Pretendard Variable, Pretendard JP first for
  `sans-serif`, Source Serif 4 / Source Han Serif KR / Noto Serif KR first for `serif`, then the
  Chromium Linux names (Arial/Arimo/Liberation Sans, Times New Roman/Tinos/Liberation Serif)
  as host fallback. No family given ⇒ `sans-serif` ⇒ Pretendard.
- **Tailwind**: generated themes declare non-inline `@theme{--font-sans:…;--font-serif:…}`
  with the canonical stacks, so `font-sans`, `font-serif` and Tailwind's
  `--default-font-family` (html) resolve to Pretendard / Source Serif 4.
- **Web delivery**: `PreparedFontCatalog.css` + `web_assets` serve only WOFF2
  (`format("woff2")`, `font-display:swap`); `native_css` + `assets` serve the TTF of the
  same faces to native (blitz build has no WOFF2 decoder; WOFF2 in native preflight is
  rejected: "Native compressed fonts require a bounded WOFF decoder adapter").

## Chrome comparison path (separate)

`meari-visual` calls `generic_fonts::use_chrome_comparison_profile()`: an unmodified headless
Chrome resolves `sans-serif`/`serif` to host Arial/Times substitutes (Arimo/Tinos/Liberation via
fontconfig), so native generics use the same host faces there. Arimo/Tinos/Liberation are never
bundled and never named in a canonical stack. Existing Tailwind fixtures keep the
`FONT_PIN` (Noto Sans) environment pin; fixtures carrying `// meari-visual: canonical-fonts`
get the bundled faces instead: Chrome via `@font-face` on the repo's upstream WOFF2
(`font-display:block` for capture determinism), native via the same faces' upstream TTF.

**Priority against host Pretendard** (`/usr/share/fonts/truetype/sand-box/custom/Pretendard`,
static OTF, family `Pretendard`): the bundled face is family `Pretendard Variable`, a distinct
name, listed first; an author `@font-face` always wins over a same-named local face in Chrome,
and native registers bundled blobs ahead of the generic list. The host static Pretendard is
reached only as the second stack entry (e.g. when bundled-fonts is disabled).

## Files, versions, checksums

| File (assets/fonts/…) | Upstream | Bytes | sha256 | Glyphs | Licence |
|---|---|---:|---|---:|---|
| pretendard/PretendardVariable.ttf | orioncactus/pretendard v1.3.9, Pretendard-1.3.9.zip `public/variable/` | 6,739,336 | 3090ccde… | 14,757 | OFL-1.1, RFN Pretendard |
| pretendard/PretendardVariable.woff2 | same zip `web/variable/woff2/` | 2,057,688 | 9599f12f… | 14,757 | OFL-1.1, RFN Pretendard |
| pretendard/PretendardJP-Regular.ttf | PretendardJP v1.3.9 (retained) | 5,462,268 | b14698ed… | 22,936 | OFL-1.1, RFN Pretendard JP |
| pretendard/PretendardJP-Regular.woff2 | PretendardJP-1.3.9.zip `web/static/woff2/` | 2,019,340 | ca8abbe5… | 22,936 | OFL-1.1, RFN Pretendard JP |
| source-serif-4/SourceSerif4Variable-Roman.ttf | adobe-fonts/source-serif 4.005R, source-serif-4.005_Desktop.zip `VAR/` | 1,204,208 | 14d360ee… | 1,464 | OFL-1.1, RFN ‘Source’ |
| source-serif-4/SourceSerif4Variable-Roman.woff2 | source-serif-4.005_WOFF2.zip `VAR/…Roman.ttf.woff2` (renamed only) | 429,100 | 940a76ed… | 1,464 | OFL-1.1, RFN ‘Source’ |

Full hashes are pinned in `crates/meari/src/domain/design/fonts.rs`. Zip sha256:
Pretendard-1.3.9.zip 04be351a74d6bf7d60c480a3087e51d185485d35a52023142af1df19eb8c428a,
PretendardJP-1.3.9.zip 8dab678c371a1530106ca643b76b2b80d47653d5ba670b01265b48e4c6615d63,
source-serif-4.005_Desktop.zip 549fdb8f9a682bd06944298621404969f6de77c2e422ff3b8244a1dcd6a0c425,
source-serif-4.005_WOFF2.zip af10e80dcd2296748b04cb9917db9f7ba0ae65101165fd2f0c16b9812d9abd28.

**Size before → after** (bundled font bytes): 10,859,564 (Pretendard Regular + SemiBold
static TTF + Pretendard JP Regular TTF) → 17,911,940 (native TTF 13,405,812 + web WOFF2
4,506,128). Measured `.git` loose objects: 12,460 KiB → 20,588 KiB (+8,128 KiB ≈ +7.9 MiB, zlib; fonts barely compress). The deleted static TTFs stay in history.
Web payload for the canonical sans: 2.06 MB WOFF2 vs 6.74 MB TTF (−69%).

## Reproduction

```
curl -LO https://github.com/orioncactus/pretendard/releases/download/v1.3.9/Pretendard-1.3.9.zip
curl -LO https://github.com/orioncactus/pretendard/releases/download/v1.3.9/PretendardJP-1.3.9.zip
curl -LO https://github.com/adobe-fonts/source-serif/releases/download/4.005R/source-serif-4.005_Desktop.zip
curl -LO https://github.com/adobe-fonts/source-serif/releases/download/4.005R/source-serif-4.005_WOFF2.zip
sha256sum *.zip   # compare with the values above
unzip -j Pretendard-1.3.9.zip public/variable/PretendardVariable.ttf web/variable/woff2/PretendardVariable.woff2
unzip -j PretendardJP-1.3.9.zip web/static/woff2/PretendardJP-Regular.woff2
unzip -j source-serif-4.005_Desktop.zip source-serif-4.005_Desktop/VAR/SourceSerif4Variable-Roman.ttf source-serif-4.005_Desktop/LICENSE.md
unzip -j source-serif-4.005_WOFF2.zip source-serif-4.005_WOFF2/VAR/SourceSerif4Variable-Roman.ttf.woff2
```

Conversion tool: **none**. All WOFF2 files are upstream-built; no local `woff2_compress` /
`pyftsubset` run. Inspection only: fontTools 4.x + brotli (glyph counts, cmap, fvar).

## OFL "Modified Version" / Reserved Font Name analysis

OFL-1.1 §1 defines a Modified Version as any derivative made by "adding to, deleting, or
substituting … any of the components of the Original Version"; §3 forbids a Modified Version
from using a Reserved Font Name without the holder's written permission. The OFL FAQ treats
subsetting as producing a Modified Version, and format conversion (e.g. to WOFF2) as a
modification unless done by the holder; it notes a web-font service may need RFN permission.
Consequence here: because every file is an unchanged upstream release (holder-produced WOFF2
included), no Modified Version exists and the Reserved Font Names (Pretendard, Pretendard JP,
‘Source’) are used as shipped. A future local subset or conversion **must be renamed** (e.g.
"Meari Sans Subset") or obtain RFN permission, and must keep the OFL notice.

## Decisions

- **unicode-range subsets**: not shipped. Local subsetting would create a Modified Version
  (rename required, above). Pretendard upstream ships per-range dynamic-subset CSS separately
  (the release zip used here contains none); adopting it is a future option.
- **font-display**: product CSS `swap` (text visible immediately, FOUT acceptable);
  comparison harness `block` (capture after the face is ready).
- **WOFF (v1) fallback**: not needed. Every browser engine in scope (Chromium, Firefox, Safari
  ≥ 12, WebView2) supports WOFF2; WOFF adds bytes without reach. Not bundled.
- **Japanese weights**: only Pretendard JP Regular is bundled (static, 5.4 MB TTF). The JP
  variable face (≈ 13 MB TTF) was rejected on size; Japanese bold is synthesized.
- **CJK serif**: Source Han Serif KR / Noto Serif KR are not bundled (≈ 20+ MB); on this box
  neither is installed, so Hangul in `serif` falls back per host.

## Tests

- `meari/tests/font_catalog.rs`: catalog exact bytes / sha256 / media types; WOFF2-only web
  CSS and TTF-only native CSS; canonical stack order (no Arimo/Tinos/Liberation); WOFF2 header
  (`wOF2`, flavor = TTF sfnt version, length, numTables ≤ TTF — upstream drops DSIG); OFL-only
  directory scan + RFN presence + THIRD_PARTY_NOTICES content.
- `meari-native/tests/bundled_font_catalog.rs`: native preflight, CSS delivery of all TTF faces,
  glyph counts and Hangul (all 11,172 syllables in Pretendard Variable) / Latin / kana / kanji
  coverage (kanji only via Pretendard JP).
- `generic_fonts` unit tests: canonical generics resolve to bundled faces first, Arial-family
  after; Chrome comparison profile keeps Chromium's choice.
- visual: `supported/canonical-font` MATCH 1.105% (tolerance 1.2%, per-line ink bbox equal).

## Limits

- WOFF2 glyph count is checked by fontTools at import time and recorded here; the Rust test
  checks the WOFF2 header against the TTF, not a decoded glyph table.
- Native never decodes the WOFF2 itself; "same font" = same upstream face, TTF container.
- No human visual check, no Windows/macOS font stack check.

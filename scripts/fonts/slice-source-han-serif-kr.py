#!/usr/bin/env python3
"""Reproducible Web slicing of Source Han Serif KR Regular (OFL-1.1) into unicode-range WOFF2.

Input : SubsetOTF/KR/SourceHanSerifKR-Regular.otf from adobe-fonts/source-han-serif 2.003R
        13_SourceHanSerifKR.zip (sha256 56a58a46812a73e2eca541ac57bca08e0c9047710bfb63ba150bedd48b2fadea).
Output: crates/meari/assets/fonts/meari-serif-kr/MeariSerifKR-Regular.<slice>.woff2 and
        crates/meari/src/application/fonts/han_serif_slices.rs (generated table).

Subsetting is an OFL-1.1 "Modified Version" and the upstream reserves the font name 'Source',
so every slice is renamed to the non-reserved family "Meari Serif KR" (name table + CFF).
Requires fonttools==4.68.0 and brotli==1.2.1. Usage: slice-source-han-serif-kr.py <otf> <repo>
"""
import hashlib
import io
import pathlib
import sys

from fontTools import subset
from fontTools.ttLib import TTFont

FAMILY = "Meari Serif KR"
PS = "MeariSerifKR"


def is_ksx1001(cp):
    try:
        return len(chr(cp).encode("euc-kr")) == 2
    except UnicodeEncodeError:
        return False


def inside(cp, ranges):
    return any(lo <= cp <= hi for lo, hi in ranges)


LATIN = [(0, 0x52F), (0x1E00, 0x1EFF), (0x2000, 0x206F), (0x20A0, 0x20CF), (0x2100, 0x218F), (0xFB00, 0xFB06)]
JAMO = [(0x1100, 0x11FF), (0x3130, 0x318F), (0xA960, 0xA97F), (0xD7B0, 0xD7FF), (0xFFA0, 0xFFDC)]
KANA = [(0x3040, 0x30FF), (0x31F0, 0x31FF), (0xFF65, 0xFF9F)]
HAN = [(0x3400, 0x4DBF), (0x4E00, 0x9FFF), (0xF900, 0xFAFF), (0x20000, 0x3FFFF)]
SYLLABLE = [(0xAC00, 0xD7A3)]
EXT_CHUNKS = 4


def classify(cmap):
    slices = {}
    rest_syllables = []
    for cp in sorted(cmap):
        if inside(cp, LATIN):
            key = "latin"
        elif inside(cp, JAMO):
            key = "hangul-jamo"
        elif inside(cp, SYLLABLE):
            if is_ksx1001(cp):
                key = "hangul-ksx1001"
            else:
                rest_syllables.append(cp)
                continue
        elif inside(cp, KANA):
            key = "kana"
        elif inside(cp, HAN):
            key = "hanja-ksx1001" if is_ksx1001(cp) else "hanja-ext"
        else:
            key = "symbols"
        slices.setdefault(key, []).append(cp)
    size = -(-len(rest_syllables) // EXT_CHUNKS)
    for i in range(EXT_CHUNKS):
        slices[f"hangul-ext-{i + 1}"] = rest_syllables[i * size:(i + 1) * size]
    order = ["latin", "hangul-jamo", "hangul-ksx1001"] + [f"hangul-ext-{i + 1}" for i in range(EXT_CHUNKS)]
    order += ["kana", "hanja-ksx1001", "hanja-ext", "symbols"]
    return [(k, slices[k]) for k in order if slices.get(k)]


def css_range(cps):
    out, start, prev = [], cps[0], cps[0]
    for cp in cps[1:] + [None]:
        if cp is not None and cp == prev + 1:
            prev = cp
            continue
        out.append(f"U+{start:X}" if start == prev else f"U+{start:X}-{prev:X}")
        if cp is not None:
            start = prev = cp
    return ",".join(out)


def rename(font):
    for record in list(font["name"].names):
        if record.nameID in (1, 3, 4, 6, 16, 17, 18, 21, 22):
            if record.langID not in (0, 0x409):
                font["name"].removeNames(nameID=record.nameID, platformID=record.platformID,
                                         platEncID=record.platEncID, langID=record.langID)
                continue
            text = record.toUnicode().replace("Source Han Serif KR", FAMILY).replace("SourceHanSerifKR", PS)
            record.string = text
    cff = font["CFF "].cff
    cff.fontNames = [n.replace("SourceHanSerifKR", PS) for n in cff.fontNames]
    top = cff.topDictIndex[0]
    for attr in ("FullName", "FamilyName"):
        if hasattr(top, attr):
            setattr(top, attr, getattr(top, attr).replace("Source Han Serif KR", FAMILY))
    for fd in getattr(top, "FDArray", []):
        if hasattr(fd, "FontName"):
            fd.FontName = fd.FontName.replace("SourceHanSerifKR", PS)


def main(otf, repo):
    repo = pathlib.Path(repo)
    out_dir = repo / "crates/meari/assets/fonts/meari-serif-kr"
    out_dir.mkdir(parents=True, exist_ok=True)
    cmap = TTFont(otf).getBestCmap()
    rows = []
    for key, cps in classify(cmap):
        font = TTFont(otf)
        options = subset.Options()
        options.flavor = "woff2"
        options.layout_features = ["*"]
        options.name_IDs = ["*"]
        options.name_languages = ["*"]
        options.notdef_outline = True
        options.drop_tables += ["DSIG"]
        sub = subset.Subsetter(options)
        sub.populate(unicodes=cps)
        sub.subset(font)
        rename(font)
        buf = io.BytesIO()
        font.flavor = "woff2"
        font.save(buf, reorderTables=False)
        data = buf.getvalue()
        name = f"MeariSerifKR-Regular.{key}.woff2"
        (out_dir / name).write_bytes(data)
        rows.append((key, name, hashlib.sha256(data).hexdigest(), len(data), len(cps), css_range(cps)))
        print(f"{name}\t{len(cps)} cps\t{len(data)} B")
    rs = repo / "crates/meari/src/application/fonts/han_serif_slices.rs"
    lines = [
        "// @generated by scripts/fonts/slice-source-han-serif-kr.py; do not edit.",
        "//! Web unicode-range slices of Source Han Serif KR Regular, renamed \"Meari Serif KR\".",
        "use crate::domain::design::fonts::WebSlice;",
        "",
        f"pub const SLICES: [WebSlice; {len(rows)}] = [",
    ]
    for key, name, sha, length, count, ranges in rows:
        lines += [
            "    WebSlice {",
            f"        name: \"{key}\",",
            f"        path: \"assets/fonts/meari-serif-kr/{name}\",",
            f"        sha256: \"{sha}\",",
            f"        length: {length},",
            f"        codepoints: {count},",
            f"        unicode_range: \"{ranges}\",",
            "    },",
        ]
    lines += ["];", "", f"pub const BYTES: [&[u8]; {len(rows)}] = ["]
    lines += [f"    include_bytes!(\"../../../assets/fonts/meari-serif-kr/{r[1]}\")," for r in rows]
    lines += ["];", ""]
    rs.write_text("\n".join(lines))


if __name__ == "__main__":
    main(*sys.argv[1:3])

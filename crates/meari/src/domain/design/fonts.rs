//! Canonical font identity: pinned Noto downloads (SIL OFL-1.1). No font bytes are compiled
//! into any binary; `meari_native::infrastructure::fonts` fetches the files below from the
//! official Google Fonts repository, verifies length and sha256, and caches them under
//! `runtime/fonts/noto/<revision>/`. The Web gets WOFF2 unicode-range slices generated from the
//! same cached files (Noto families carry no Reserved Font Name, so subsets keep the name).

/// Official Google Fonts repository revision the pins were taken from (2026-10-30).
pub const REVISION: &str = "6157dfaa640ca5738b4d1a7070fe389c3036d7fc";

/// One pinned upstream file plus its directory's OFL notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pin {
    pub script: &'static str,
    pub family: &'static str,
    pub dir: &'static str,
    pub file: &'static str,
    pub style: &'static str,
    pub length: u64,
    pub sha256: &'static str,
    pub ofl_length: u64,
    pub ofl_sha256: &'static str,
}

/// Script -> Noto family manifest (sans first, then serif). `latin` covers Latin, Greek and
/// Cyrillic; `hangul` also covers KS X 1001 hanja; `kana` covers kana and JIS kanji.
pub const MANIFEST: &[Pin] = &[
    Pin {
        script: "latin",
        family: "Noto Sans",
        dir: "notosans",
        file: "NotoSans[wdth,wght].ttf",
        style: "normal",
        length: 2049096,
        sha256: "bfb7bb691513f12e734dc346c03a03f784912432d7e3fa8e56efcf906fe86b3d",
        ofl_length: 4396,
        ofl_sha256: "cee9892f9f0cc8fe882c9e9537ee6a89621d86ee7ceaf70b02e2b2b1c25c061a",
    },
    Pin {
        script: "latin",
        family: "Noto Sans",
        dir: "notosans",
        file: "NotoSans-Italic[wdth,wght].ttf",
        style: "italic",
        length: 2322640,
        sha256: "58e6e0ebd1931b29a365aa2d3e2ee9a9e831a3af7cf3ad1462d4e72154f0b291",
        ofl_length: 4396,
        ofl_sha256: "cee9892f9f0cc8fe882c9e9537ee6a89621d86ee7ceaf70b02e2b2b1c25c061a",
    },
    Pin {
        script: "latin",
        family: "Noto Serif",
        dir: "notoserif",
        file: "NotoSerif[wdth,wght].ttf",
        style: "normal",
        length: 1887192,
        sha256: "4d8e6761424656867019081a1a01336f3cb086982682698714054fc33f782713",
        ofl_length: 4396,
        ofl_sha256: "cee9892f9f0cc8fe882c9e9537ee6a89621d86ee7ceaf70b02e2b2b1c25c061a",
    },
    Pin {
        script: "latin",
        family: "Noto Serif",
        dir: "notoserif",
        file: "NotoSerif-Italic[wdth,wght].ttf",
        style: "italic",
        length: 2448496,
        sha256: "e87acbc6c0efd0d9a20d6a8cbbda2b266c14be3a3a6f5af8ec9d7b2460570ad1",
        ofl_length: 4396,
        ofl_sha256: "cee9892f9f0cc8fe882c9e9537ee6a89621d86ee7ceaf70b02e2b2b1c25c061a",
    },
    Pin {
        script: "latin",
        family: "Noto Sans Mono",
        dir: "notosansmono",
        file: "NotoSansMono[wdth,wght].ttf",
        style: "normal",
        length: 1708408,
        sha256: "2cb2adb378a8f574213e23df697050b83c54c27df465a2015552740b2769a081",
        ofl_length: 4396,
        ofl_sha256: "cee9892f9f0cc8fe882c9e9537ee6a89621d86ee7ceaf70b02e2b2b1c25c061a",
    },
    Pin {
        script: "hangul",
        family: "Noto Sans KR",
        dir: "notosanskr",
        file: "NotoSansKR[wght].ttf",
        style: "normal",
        length: 10414588,
        sha256: "194018e6b2b293a7964f037b25c0249ce1418bc9ab3c971060a03aa57861e252",
        ofl_length: 4388,
        ofl_sha256: "1c05c68c34f9708415aada51f17e1b0092d2cea709bf4a94cd38114f9e73d7d9",
    },
    Pin {
        script: "hangul",
        family: "Noto Serif KR",
        dir: "notoserifkr",
        file: "NotoSerifKR[wght].ttf",
        style: "normal",
        length: 23795420,
        sha256: "11f8d5de6f1b79195efba3828aaa2ec95c1178f5ae976fb23c8d53250a9938f3",
        ofl_length: 4350,
        ofl_sha256: "5e0da210fb04058a8c0087985d2d456b931c2579811a49655721d3cf0c36b6d6",
    },
    Pin {
        script: "kana",
        family: "Noto Sans JP",
        dir: "notosansjp",
        file: "NotoSansJP[wght].ttf",
        style: "normal",
        length: 9589900,
        sha256: "c2f3b4d463500a2ddcd3849cded1fceeb9fd6d1c32e6cbecd568453ba50fc68f",
        ofl_length: 4388,
        ofl_sha256: "1c05c68c34f9708415aada51f17e1b0092d2cea709bf4a94cd38114f9e73d7d9",
    },
    Pin {
        script: "kana",
        family: "Noto Serif JP",
        dir: "notoserifjp",
        file: "NotoSerifJP[wght].ttf",
        style: "normal",
        length: 13574352,
        sha256: "2fd527ba12b6a44ec30d796d633360da0aeba6c5d4af1304ce12bb4dc15a7dfc",
        ofl_length: 4350,
        ofl_sha256: "5e0da210fb04058a8c0087985d2d456b931c2579811a49655721d3cf0c36b6d6",
    },
    Pin {
        script: "arabic",
        family: "Noto Sans Arabic",
        dir: "notosansarabic",
        file: "NotoSansArabic[wdth,wght].ttf",
        style: "normal",
        length: 844676,
        sha256: "63111b5b2e074dd48cc67692e0a2726d86ee94c1c37fe8598257b7b4e87e869e",
        ofl_length: 4382,
        ofl_sha256: "07fc70bfeb985cc1a87a8587d0a0c80bab11c86c9dc3fd95b6f0cb332f983e96",
    },
    Pin {
        script: "arabic",
        family: "Noto Naskh Arabic",
        dir: "notonaskharabic",
        file: "NotoNaskhArabic[wght].ttf",
        style: "normal",
        length: 307592,
        sha256: "67b5a525a661b607971fbd3f96a81b89d3a768e74534fca84f18ac97e6fab72f",
        ofl_length: 4382,
        ofl_sha256: "a7a5a25eb188bf1cd96982030d53e23c33485c69b1044a562254226857ee13af",
    },
    Pin {
        script: "hebrew",
        family: "Noto Sans Hebrew",
        dir: "notosanshebrew",
        file: "NotoSansHebrew[wdth,wght].ttf",
        style: "normal",
        length: 112640,
        sha256: "7ef36a2c3593758cdb622e1bdef4f84523e92fbc3ccc667438dd80ff54c2de88",
        ofl_length: 4382,
        ofl_sha256: "9b9fe028b5ba74d231659a1bbaf0ed09b11e759d1ca6a070999e16d151616b47",
    },
    Pin {
        script: "hebrew",
        family: "Noto Serif Hebrew",
        dir: "notoserifhebrew",
        file: "NotoSerifHebrew[wdth,wght].ttf",
        style: "normal",
        length: 183600,
        sha256: "93caef921360788dc3b0e32136bb26f16bc57717ec482d48fc7fd43820617165",
        ofl_length: 4382,
        ofl_sha256: "9b9fe028b5ba74d231659a1bbaf0ed09b11e759d1ca6a070999e16d151616b47",
    },
    Pin {
        script: "devanagari",
        family: "Noto Sans Devanagari",
        dir: "notosansdevanagari",
        file: "NotoSansDevanagari[wdth,wght].ttf",
        style: "normal",
        length: 641944,
        sha256: "14ec4af41f27482216d1c2229f417ff9b1425e1babb014e57d1d40d03229853e",
        ofl_length: 4386,
        ofl_sha256: "a216f6f8d85c7228093e0ee5e258d9d377e6671f68acb4db1930b29583d0f331",
    },
    Pin {
        script: "devanagari",
        family: "Noto Serif Devanagari",
        dir: "notoserifdevanagari",
        file: "NotoSerifDevanagari[wdth,wght].ttf",
        style: "normal",
        length: 757692,
        sha256: "1191e07bfeb062d80e252eb85b0eafdfbda1e350707a2a60628668e8f677dbbb",
        ofl_length: 4386,
        ofl_sha256: "a216f6f8d85c7228093e0ee5e258d9d377e6671f68acb4db1930b29583d0f331",
    },
    Pin {
        script: "thai",
        family: "Noto Sans Thai",
        dir: "notosansthai",
        file: "NotoSansThai[wdth,wght].ttf",
        style: "normal",
        length: 218652,
        sha256: "5a1c559bb539583c8a1fd99d1c5b9491e5e14478c9cd2bd0970d5c3096cc9ef8",
        ofl_length: 4380,
        ofl_sha256: "2e98fd23a52d253db8612cd5942c8f2ff4111b21d2367050fdca91d8ccc374a0",
    },
    Pin {
        script: "thai",
        family: "Noto Serif Thai",
        dir: "notoserifthai",
        file: "NotoSerifThai[wdth,wght].ttf",
        style: "normal",
        length: 292188,
        sha256: "34a7ad11647c845303aabdde639059806c56b84719e5d2ceb28eb038711bdf53",
        ofl_length: 4380,
        ofl_sha256: "2e98fd23a52d253db8612cd5942c8f2ff4111b21d2367050fdca91d8ccc374a0",
    },
];

impl Pin {
    pub fn url(&self) -> String {
        raw_url(self.dir, self.file)
    }
    pub fn ofl_url(&self) -> String {
        raw_url(self.dir, "OFL.txt")
    }
    /// Variable `wght` range of every pinned face.
    pub fn weight(&self) -> &'static str {
        "100 900"
    }
}

fn raw_url(dir: &str, file: &str) -> String {
    let file = file
        .replace('[', "%5B")
        .replace(']', "%5D")
        .replace(',', "%2C");
    format!("https://raw.githubusercontent.com/google/fonts/{REVISION}/ofl/{dir}/{file}")
}

/// Pins for the requested scripts (`all` selects every script); unknown names are an error.
pub fn select(scripts: &[&str]) -> Result<Vec<Pin>, String> {
    for script in scripts {
        if *script != "all" && !MANIFEST.iter().any(|p| p.script == *script) {
            return Err(format!(
                "unknown font script `{script}`; known: all, {}",
                known_scripts().join(", ")
            ));
        }
    }
    Ok(MANIFEST
        .iter()
        .filter(|p| scripts.iter().any(|s| *s == "all" || *s == p.script))
        .copied()
        .collect())
}

pub fn known_scripts() -> Vec<&'static str> {
    let mut out: Vec<&str> = Vec::new();
    for pin in MANIFEST {
        if !out.contains(&pin.script) {
            out.push(pin.script);
        }
    }
    out
}

/// Scripts of the canonical UI faces (Latin + Hangul + Japanese, sans and serif).
pub const UI_SCRIPTS: &[&str] = &["latin", "hangul", "kana"];

/// Canonical sans-serif stack: Noto per script, then the generic (no host face is named).
pub const UI_FONT_FAMILY: &str = "\"Noto Sans\", \"Noto Sans KR\", \"Noto Sans JP\", \"Noto Sans Arabic\", \"Noto Sans Hebrew\", \"Noto Sans Devanagari\", \"Noto Sans Thai\", sans-serif";
pub const SANS_FONT_FAMILY: &str = UI_FONT_FAMILY;
/// Canonical serif stack: Noto Serif per script (Arabic: Noto Naskh Arabic), then the generic.
pub const SERIF_FONT_FAMILY: &str = "\"Noto Serif\", \"Noto Serif KR\", \"Noto Serif JP\", \"Noto Naskh Arabic\", \"Noto Serif Hebrew\", \"Noto Serif Devanagari\", \"Noto Serif Thai\", serif";
pub const MONO_FONT_FAMILY: &str =
    "\"Noto Sans Mono\", \"Noto Sans KR\", \"Noto Sans JP\", monospace";

/// Families the native profiles put first for `sans-serif` / `serif` / `monospace`.
pub const SANS_FAMILIES: [&str; 7] = [
    "Noto Sans",
    "Noto Sans KR",
    "Noto Sans JP",
    "Noto Sans Arabic",
    "Noto Sans Hebrew",
    "Noto Sans Devanagari",
    "Noto Sans Thai",
];
pub const SERIF_FAMILIES: [&str; 7] = [
    "Noto Serif",
    "Noto Serif KR",
    "Noto Serif JP",
    "Noto Naskh Arabic",
    "Noto Serif Hebrew",
    "Noto Serif Devanagari",
    "Noto Serif Thai",
];
pub const MONO_FAMILIES: [&str; 3] = ["Noto Sans Mono", "Noto Sans KR", "Noto Sans JP"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontRequirement {
    PinnedAssets,
    NoSystemFallback,
    VerifiedGlyphCoverage,
    DeterministicRaster,
}

/// Host fallback, decode, chosen faces, shaping and output pixels remain unverified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontSupport {
    PinnedAssetsOnly,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_covers_the_core_scripts_with_sans_and_serif() {
        for script in [
            "latin",
            "hangul",
            "kana",
            "arabic",
            "hebrew",
            "devanagari",
            "thai",
        ] {
            let pins = select(&[script]).unwrap();
            assert!(pins.iter().any(|p| SANS_FAMILIES.contains(&p.family)));
            assert!(pins.iter().any(|p| SERIF_FAMILIES.contains(&p.family)));
        }
        assert!(
            MANIFEST
                .iter()
                .all(|p| p.sha256.len() == 64 && p.ofl_sha256.len() == 64)
        );
        assert!(select(&["klingon"]).is_err());
        assert_eq!(select(&["all"]).unwrap().len(), MANIFEST.len());
    }

    #[test]
    fn urls_point_at_the_pinned_official_revision() {
        let pin = select(&["latin"]).unwrap()[0];
        assert_eq!(
            pin.url(),
            format!(
                "https://raw.githubusercontent.com/google/fonts/{REVISION}/ofl/notosans/NotoSans%5Bwdth%2Cwght%5D.ttf"
            )
        );
    }

    #[test]
    fn every_stack_family_is_pinned() {
        for family in SANS_FAMILIES
            .iter()
            .chain(&SERIF_FAMILIES)
            .chain(&MONO_FAMILIES)
        {
            assert!(MANIFEST.iter().any(|p| p.family == *family), "{family}");
        }
    }
}

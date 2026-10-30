//! Pinned Noto font downloads (OFL-1.1, no Reserved Font Name), fetched like the pinned
//! Tailwind binary: fixed upstream revision, URL, length and sha256; cached under
//! `runtime/fonts/noto/<revision>/`; a valid cache is used offline; a length or checksum
//! mismatch is an error (never skipped). No font bytes are compiled into any binary.
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Official Google Fonts repository revision the pins were taken from (2026-10-30).
pub const REVISION: &str = "6157dfaa640ca5738b4d1a7070fe389c3036d7fc";
/// Scripts the visual test needs (FONT_PIN family "Noto Sans", Roman + Italic, and Noto Serif).
pub const VISUAL_TEST_SCRIPTS: &[&str] = &["latin"];

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

/// Cache directory for this revision.
pub fn cache_dir(repo_root: &Path) -> PathBuf {
    repo_root.join("runtime/fonts/noto").join(&REVISION[..12])
}

/// Pins for the requested scripts (`all` selects every script); unknown names are an error.
pub fn select(scripts: &[&str]) -> Result<Vec<Pin>, String> {
    for script in scripts {
        if *script != "all" && !MANIFEST.iter().any(|p| p.script == *script) {
            return Err(format!(
                "unknown font script `{script}`; known: {}",
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

/// Ensures every selected file (and its OFL notice) is cached and verified; returns the
/// font paths in manifest order. Downloads only what is missing.
pub fn ensure(scripts: &[&str], repo_root: &Path) -> Result<Vec<(Pin, PathBuf)>, String> {
    let dir = cache_dir(repo_root);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut out = Vec::new();
    for pin in select(scripts)? {
        let path = dir.join(pin.file);
        fetch_verified(&pin.url(), &path, pin.length, pin.sha256)?;
        let notice = dir.join(format!("{}-OFL.txt", pin.dir));
        fetch_verified(&pin.ofl_url(), &notice, pin.ofl_length, pin.ofl_sha256)?;
        out.push((pin, path));
    }
    Ok(out)
}

/// Checks `bytes` against a pin; the error names the expected and actual values.
pub fn verify(name: &str, bytes: &[u8], length: u64, sha256: &str) -> Result<(), String> {
    let actual = hex(&Sha256::digest(bytes));
    if bytes.len() as u64 != length || actual != sha256 {
        return Err(format!(
            "font checksum mismatch for {name}: expected {length} bytes sha256 {sha256}, got {} bytes sha256 {actual}; delete the file and run `cargo xtask fonts-fetch` again",
            bytes.len()
        ));
    }
    Ok(())
}

fn fetch_verified(url: &str, path: &Path, length: u64, sha256: &str) -> Result<(), String> {
    let name = path.display().to_string();
    if let Ok(bytes) = std::fs::read(path) {
        return verify(&name, &bytes, length, sha256);
    }
    let part = path.with_extension("part");
    let status = std::process::Command::new("curl")
        .args(["-fsSL", "--retry", "2", "-o"])
        .arg(&part)
        .arg(url)
        .status()
        .map_err(|e| format!("cannot run curl to fetch {url}: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&part);
        return Err(format!(
            "font download failed ({status}) for {url} and no cached copy at {name} (offline?)"
        ));
    }
    let bytes = std::fs::read(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    if let Err(e) = verify(url, &bytes, length, sha256) {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, path).map_err(|e| format!("{name}: {e}"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
            assert!(
                pins.iter()
                    .any(|p| !p.family.contains("Serif") && !p.family.contains("Naskh"))
            );
            assert!(
                pins.iter()
                    .any(|p| p.family.contains("Serif") || p.family.contains("Naskh"))
            );
        }
        assert!(
            MANIFEST
                .iter()
                .all(|p| p.sha256.len() == 64 && p.ofl_sha256.len() == 64)
        );
        assert!(select(&["klingon"]).is_err());
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
    fn checksum_mismatch_is_a_clear_error() {
        let err = verify("x.ttf", b"abc", 3, MANIFEST[0].sha256).unwrap_err();
        assert!(err.contains("checksum mismatch for x.ttf"), "{err}");
        verify(
            "abc",
            b"abc",
            3,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .unwrap();
    }
}

//! Canonical bundled font identity (all SIL OFL-1.1); coverage and raster quality are separate
//! host contracts. Every face is an unchanged upstream release file: native consumes the
//! TTF, the Web consumes the upstream-built WOFF2 of the same face (no local subsetting or
//! conversion, so no OFL "Modified Version" / Reserved Font Name question arises).
pub const UPSTREAM_RELEASE: &str = "v1.3.9";
pub const UPSTREAM_RELEASE_URL: &str =
    "https://github.com/orioncactus/pretendard/releases/tag/v1.3.9";

/// Canonical sans-serif stack: bundled Pretendard Variable, then an installed static
/// Pretendard, bundled Pretendard JP for kana/kanji, then the prior Arial-family fallback.
pub const UI_FONT_FAMILY: &str =
    "\"Pretendard Variable\", Pretendard, \"Pretendard JP\", Arial, sans-serif";
pub const SANS_FONT_FAMILY: &str = UI_FONT_FAMILY;
/// Canonical serif stack: bundled Source Serif 4, then system CJK serif (not bundled).
pub const SERIF_FONT_FAMILY: &str = "\"Source Serif 4 Variable\", \"Source Serif 4\", \"Source Han Serif KR\", \"Noto Serif KR\", \"Times New Roman\", serif";

/// Families the native canonical profile puts first for `sans-serif` / `serif`.
pub const SANS_FAMILIES: [&str; 2] = ["Pretendard Variable", "Pretendard JP"];
pub const SERIF_FAMILIES: [&str; 3] = [
    "Source Serif 4 Variable",
    "Source Han Serif KR",
    "Noto Serif KR",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontFace {
    PretendardVariable,
    PretendardJpRegular,
    SourceSerif4Variable,
}

pub const UI_FACES: [FontFace; 3] = [
    FontFace::PretendardVariable,
    FontFace::PretendardJpRegular,
    FontFace::SourceSerif4Variable,
];

/// Container of one face file: native reads `Truetype`, the Web is served `Woff2`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontFormat {
    Truetype,
    Woff2,
}

impl FontFormat {
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Truetype => "font/ttf",
            Self::Woff2 => "font/woff2",
        }
    }
    pub fn css(self) -> &'static str {
        match self {
            Self::Truetype => "truetype",
            Self::Woff2 => "woff2",
        }
    }
}

/// (path, sha256, length) of one upstream file.
pub struct FontFile {
    pub path: &'static str,
    pub sha256: &'static str,
    pub length: u64,
}

impl FontFace {
    pub fn file(self, format: FontFormat) -> FontFile {
        use FontFormat::*;
        let (path, sha256, length) = match (self, format) {
            (Self::PretendardVariable, Truetype) => (
                "assets/fonts/pretendard/PretendardVariable.ttf",
                "3090ccde0442bb347aa7685d9ba8b17436a60682df6e8f92a9a670de14056e22",
                6_739_336,
            ),
            (Self::PretendardVariable, Woff2) => (
                "assets/fonts/pretendard/PretendardVariable.woff2",
                "9599f12fd42fc0bce1cd50b47a0c022e108d7aa64dd0d1bb0ed44f3282d900b4",
                2_057_688,
            ),
            (Self::PretendardJpRegular, Truetype) => (
                "assets/fonts/pretendard/PretendardJP-Regular.ttf",
                "b14698ed6d82737f45e918d079bca6fc498a107bcb094b56c5229dbddf3246d3",
                5_462_268,
            ),
            (Self::PretendardJpRegular, Woff2) => (
                "assets/fonts/pretendard/PretendardJP-Regular.woff2",
                "ca8abbe573f57f235faa68525e427646532c7aa5ab65d0c267c179e9a0ff76b8",
                2_019_340,
            ),
            (Self::SourceSerif4Variable, Truetype) => (
                "assets/fonts/source-serif-4/SourceSerif4Variable-Roman.ttf",
                "14d360ee1b76655da9276628b229e11671bc1f5d1083636144db6677d452cf55",
                1_204_208,
            ),
            (Self::SourceSerif4Variable, Woff2) => (
                "assets/fonts/source-serif-4/SourceSerif4Variable-Roman.woff2",
                "940a76eda1388de39d38c8e7a79bf6ea058a387faee0a9f33c8d25c6ba05e1be",
                429_100,
            ),
        };
        FontFile {
            path,
            sha256,
            length,
        }
    }
    pub fn source_path(self) -> &'static str {
        self.file(FontFormat::Truetype).path
    }
    pub fn sha256(self) -> &'static str {
        self.file(FontFormat::Truetype).sha256
    }
    pub fn length(self) -> u64 {
        self.file(FontFormat::Truetype).length
    }
    pub fn family(self) -> &'static str {
        match self {
            Self::PretendardVariable => "Pretendard Variable",
            Self::PretendardJpRegular => "Pretendard JP",
            Self::SourceSerif4Variable => "Source Serif 4 Variable",
        }
    }
    /// CSS `font-weight` descriptor: the variable `wght` axis range, or the static weight.
    pub fn weight(self) -> &'static str {
        match self {
            Self::PretendardVariable => "45 930",
            Self::PretendardJpRegular => "400",
            Self::SourceSerif4Variable => "200 900",
        }
    }
    /// OS/2 usWeightClass of the default instance.
    pub fn default_weight(self) -> u16 {
        400
    }
    pub fn license(self) -> FontLicense {
        match self {
            Self::PretendardVariable => FontLicense::Pretendard,
            Self::PretendardJpRegular => FontLicense::PretendardJp,
            Self::SourceSerif4Variable => FontLicense::SourceSerif4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontLicense {
    Pretendard,
    PretendardJp,
    SourceSerif4,
}

pub const LICENSES: [FontLicense; 3] = [
    FontLicense::Pretendard,
    FontLicense::PretendardJp,
    FontLicense::SourceSerif4,
];

impl FontLicense {
    pub fn path(self) -> &'static str {
        match self {
            Self::Pretendard => "assets/fonts/pretendard/LICENSE.txt",
            Self::PretendardJp => "assets/fonts/pretendard/LICENSE-JP.txt",
            Self::SourceSerif4 => "assets/fonts/source-serif-4/LICENSE.md",
        }
    }
    pub fn sha256(self) -> &'static str {
        match self {
            Self::Pretendard => "b04538c9abec39a3db75108cf0af0fd9c77032fe8aa2cf38345b4d250e98e38e",
            Self::PretendardJp => {
                "6cb2adce04090e5e321168d5b47e12d1cb703da64929a4f34dba9162eb55eb98"
            }
            Self::SourceSerif4 => {
                "75784a295293a8992f5a8d99210566e0064a012e6dab6731305e3787f15896c7"
            }
        }
    }
    /// Reserved Font Names declared by the upstream copyright line.
    pub fn reserved_names(self) -> &'static [&'static str] {
        match self {
            Self::Pretendard => &["Pretendard"],
            Self::PretendardJp => &["Pretendard JP"],
            Self::SourceSerif4 => &["Source"],
        }
    }
}

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

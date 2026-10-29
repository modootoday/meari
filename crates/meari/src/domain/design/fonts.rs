//! Canonical bundled font identity (all SIL OFL-1.1); coverage and raster quality are separate
//! host contracts. Native consumes unchanged upstream TTF/OTF. The Web consumes the
//! upstream-built WOFF2 of the same face, except Source Han Serif KR, which is served as
//! unicode-range WOFF2 slices: a subset is an OFL "Modified Version" and the upstream reserves
//! the name 'Source', so the slices carry the non-reserved family [`MEARI_SERIF_KR`].
pub const UPSTREAM_RELEASE: &str = "v1.3.9";
pub const UPSTREAM_RELEASE_URL: &str =
    "https://github.com/orioncactus/pretendard/releases/tag/v1.3.9";

/// Renamed family of the Web slices of Source Han Serif KR (see `scripts/fonts`).
pub const MEARI_SERIF_KR: &str = "Meari Serif KR";

/// Canonical sans-serif stack: bundled Pretendard Variable, an installed static Pretendard,
/// bundled Pretendard JP for kana/kanji, then host fallbacks for macOS, Windows and Linux
/// (code/CSS level only; only Linux is executed in CI).
pub const UI_FONT_FAMILY: &str = "\"Pretendard Variable\", Pretendard, \"Pretendard JP\", -apple-system, BlinkMacSystemFont, \"Apple SD Gothic Neo\", \"Hiragino Sans\", \"Segoe UI\", \"Malgun Gothic\", \"Yu Gothic UI\", \"Noto Sans KR\", \"Noto Sans CJK KR\", Arial, sans-serif";
pub const SANS_FONT_FAMILY: &str = UI_FONT_FAMILY;
/// Canonical serif stack: bundled Source Serif 4 (Latin), bundled Source Han Serif KR
/// (native) / Meari Serif KR (Web slices) for Hangul, hanja and kana, then host fallbacks.
pub const SERIF_FONT_FAMILY: &str = "\"Source Serif 4 Variable\", \"Source Serif 4\", \"Source Han Serif KR\", \"Meari Serif KR\", \"Noto Serif KR\", \"Noto Serif CJK KR\", AppleMyungjo, \"Hiragino Mincho ProN\", Batang, \"Yu Mincho\", \"Times New Roman\", serif";

/// Families the native canonical profile puts first for `sans-serif` / `serif`.
pub const SANS_FAMILIES: [&str; 2] = ["Pretendard Variable", "Pretendard JP"];
pub const SERIF_FAMILIES: [&str; 3] = [
    "Source Serif 4 Variable",
    "Source Han Serif KR",
    "Noto Serif KR",
];

/// Host fallbacks per OS after the bundled faces (data only; the platform adapter selects).
pub struct HostFallbacks {
    pub sans: &'static [&'static str],
    pub serif: &'static [&'static str],
}
pub const MACOS_FALLBACKS: HostFallbacks = HostFallbacks {
    sans: &["Apple SD Gothic Neo", "Hiragino Sans"],
    serif: &["AppleMyungjo", "Hiragino Mincho ProN", "Times New Roman"],
};
pub const WINDOWS_FALLBACKS: HostFallbacks = HostFallbacks {
    sans: &["Segoe UI", "Malgun Gothic", "Yu Gothic UI", "Arial"],
    serif: &["Batang", "Yu Mincho", "Times New Roman"],
};
pub const LINUX_FALLBACKS: HostFallbacks = HostFallbacks {
    sans: &["Noto Sans CJK KR", "Noto Sans KR"],
    serif: &["Noto Serif CJK KR", "Noto Serif KR"],
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontFace {
    PretendardVariable,
    PretendardJpRegular,
    PretendardJpBold,
    SourceSerif4Variable,
    SourceHanSerifKrRegular,
}

pub const UI_FACES: [FontFace; 5] = [
    FontFace::PretendardVariable,
    FontFace::PretendardJpRegular,
    FontFace::PretendardJpBold,
    FontFace::SourceSerif4Variable,
    FontFace::SourceHanSerifKrRegular,
];

/// Container of one face file: native reads `Truetype`/`Opentype`, the Web is served `Woff2`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontFormat {
    Truetype,
    Opentype,
    Woff2,
}

impl FontFormat {
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Truetype => "font/ttf",
            Self::Opentype => "font/otf",
            Self::Woff2 => "font/woff2",
        }
    }
    pub fn css(self) -> &'static str {
        match self {
            Self::Truetype => "truetype",
            Self::Opentype => "opentype",
            Self::Woff2 => "woff2",
        }
    }
}

/// (path, sha256, length, container) of one bundled file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontFile {
    pub path: &'static str,
    pub sha256: &'static str,
    pub length: u64,
    pub format: FontFormat,
}

/// One renamed unicode-range WOFF2 slice of a modified (subset) face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WebSlice {
    pub name: &'static str,
    pub path: &'static str,
    pub sha256: &'static str,
    pub length: u64,
    pub codepoints: usize,
    pub unicode_range: &'static str,
}

impl FontFace {
    /// Unchanged upstream file read by native.
    pub fn native_file(self) -> FontFile {
        use FontFormat::*;
        let (path, sha256, length, format) = match self {
            Self::PretendardVariable => (
                "assets/fonts/pretendard/PretendardVariable.ttf",
                "3090ccde0442bb347aa7685d9ba8b17436a60682df6e8f92a9a670de14056e22",
                6_739_336,
                Truetype,
            ),
            Self::PretendardJpRegular => (
                "assets/fonts/pretendard/PretendardJP-Regular.ttf",
                "b14698ed6d82737f45e918d079bca6fc498a107bcb094b56c5229dbddf3246d3",
                5_462_268,
                Truetype,
            ),
            Self::PretendardJpBold => (
                "assets/fonts/pretendard/PretendardJP-Bold.ttf",
                "b6f14d5848f95bc50f28c323234574a8357fdd992582cd98acbfc3b33da44255",
                5_412_352,
                Truetype,
            ),
            Self::SourceSerif4Variable => (
                "assets/fonts/source-serif-4/SourceSerif4Variable-Roman.ttf",
                "14d360ee1b76655da9276628b229e11671bc1f5d1083636144db6677d452cf55",
                1_204_208,
                Truetype,
            ),
            Self::SourceHanSerifKrRegular => (
                "assets/fonts/source-han-serif-kr/SourceHanSerifKR-Regular.otf",
                "b2b310255186ce92de2909f96522b9183a9f6c323dbe5663f205f02d6ece08b7",
                7_669_328,
                Opentype,
            ),
        };
        FontFile {
            path,
            sha256,
            length,
            format,
        }
    }
    /// Unchanged upstream WOFF2, or `None` when the Web gets renamed slices instead.
    pub fn web_woff2(self) -> Option<FontFile> {
        let (path, sha256, length) = match self {
            Self::PretendardVariable => (
                "assets/fonts/pretendard/PretendardVariable.woff2",
                "9599f12fd42fc0bce1cd50b47a0c022e108d7aa64dd0d1bb0ed44f3282d900b4",
                2_057_688,
            ),
            Self::PretendardJpRegular => (
                "assets/fonts/pretendard/PretendardJP-Regular.woff2",
                "ca8abbe573f57f235faa68525e427646532c7aa5ab65d0c267c179e9a0ff76b8",
                2_019_340,
            ),
            Self::PretendardJpBold => (
                "assets/fonts/pretendard/PretendardJP-Bold.woff2",
                "3c926898801c5ef97af081151566f7038ac529de88c77d212ea5f2037309dad2",
                2_107_244,
            ),
            Self::SourceSerif4Variable => (
                "assets/fonts/source-serif-4/SourceSerif4Variable-Roman.woff2",
                "940a76eda1388de39d38c8e7a79bf6ea058a387faee0a9f33c8d25c6ba05e1be",
                429_100,
            ),
            Self::SourceHanSerifKrRegular => return None,
        };
        Some(FontFile {
            path,
            sha256,
            length,
            format: FontFormat::Woff2,
        })
    }
    pub fn source_path(self) -> &'static str {
        self.native_file().path
    }
    pub fn sha256(self) -> &'static str {
        self.native_file().sha256
    }
    pub fn length(self) -> u64 {
        self.native_file().length
    }
    /// Family name inside the native file.
    pub fn family(self) -> &'static str {
        match self {
            Self::PretendardVariable => "Pretendard Variable",
            Self::PretendardJpRegular | Self::PretendardJpBold => "Pretendard JP",
            Self::SourceSerif4Variable => "Source Serif 4 Variable",
            Self::SourceHanSerifKrRegular => "Source Han Serif KR",
        }
    }
    /// Family name of the Web delivery (renamed for modified slices).
    pub fn web_family(self) -> &'static str {
        match self {
            Self::SourceHanSerifKrRegular => MEARI_SERIF_KR,
            other => other.family(),
        }
    }
    /// CSS `font-weight` descriptor: the variable `wght` axis range, or the static weight.
    pub fn weight(self) -> &'static str {
        match self {
            Self::PretendardVariable => "45 930",
            Self::PretendardJpRegular | Self::SourceHanSerifKrRegular => "400",
            Self::PretendardJpBold => "700",
            Self::SourceSerif4Variable => "200 900",
        }
    }
    /// OS/2 usWeightClass of the default instance.
    pub fn default_weight(self) -> u16 {
        match self {
            Self::PretendardJpBold => 700,
            _ => 400,
        }
    }
    pub fn license(self) -> FontLicense {
        match self {
            Self::PretendardVariable => FontLicense::Pretendard,
            Self::PretendardJpRegular | Self::PretendardJpBold => FontLicense::PretendardJp,
            Self::SourceSerif4Variable => FontLicense::SourceSerif4,
            Self::SourceHanSerifKrRegular => FontLicense::SourceHanSerifKr,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontLicense {
    Pretendard,
    PretendardJp,
    SourceSerif4,
    SourceHanSerifKr,
    /// The same Adobe OFL notice, shipped beside the renamed Web slices.
    MeariSerifKr,
}

pub const LICENSES: [FontLicense; 5] = [
    FontLicense::Pretendard,
    FontLicense::PretendardJp,
    FontLicense::SourceSerif4,
    FontLicense::SourceHanSerifKr,
    FontLicense::MeariSerifKr,
];

impl FontLicense {
    pub fn path(self) -> &'static str {
        match self {
            Self::Pretendard => "assets/fonts/pretendard/LICENSE.txt",
            Self::PretendardJp => "assets/fonts/pretendard/LICENSE-JP.txt",
            Self::SourceSerif4 => "assets/fonts/source-serif-4/LICENSE.md",
            Self::SourceHanSerifKr => "assets/fonts/source-han-serif-kr/LICENSE.txt",
            Self::MeariSerifKr => "assets/fonts/meari-serif-kr/LICENSE.txt",
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
            Self::SourceHanSerifKr | Self::MeariSerifKr => {
                "9ff5bb567e1b92c801fc1069e5fbf992ff8efccacb9db94e5959a5b3ba9bb903"
            }
        }
    }
    /// Reserved Font Names declared by the upstream copyright line.
    pub fn reserved_names(self) -> &'static [&'static str] {
        match self {
            Self::Pretendard => &["Pretendard"],
            Self::PretendardJp => &["Pretendard JP"],
            Self::SourceSerif4 | Self::SourceHanSerifKr | Self::MeariSerifKr => &["Source"],
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

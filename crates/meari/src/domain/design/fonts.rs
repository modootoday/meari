//! Retained font identity; coverage and raster quality are separate host contracts.
pub const UPSTREAM_RELEASE: &str = "v1.3.9";
pub const UPSTREAM_RELEASE_URL: &str =
    "https://github.com/orioncactus/pretendard/releases/tag/v1.3.9";
pub const UI_FONT_FAMILY: &str = "Pretendard, \"Pretendard JP\", sans-serif";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontFace {
    PretendardRegular,
    PretendardSemiBold,
    PretendardJpRegular,
}

pub const UI_FACES: [FontFace; 3] = [
    FontFace::PretendardRegular,
    FontFace::PretendardSemiBold,
    FontFace::PretendardJpRegular,
];

impl FontFace {
    pub fn source_path(self) -> &'static str {
        match self {
            Self::PretendardRegular => "assets/fonts/pretendard/Pretendard-Regular.ttf",
            Self::PretendardSemiBold => "assets/fonts/pretendard/Pretendard-SemiBold.ttf",
            Self::PretendardJpRegular => "assets/fonts/pretendard/PretendardJP-Regular.ttf",
        }
    }
    pub fn family(self) -> &'static str {
        match self {
            Self::PretendardRegular | Self::PretendardSemiBold => "Pretendard",
            Self::PretendardJpRegular => "Pretendard JP",
        }
    }
    pub fn weight(self) -> u16 {
        match self {
            Self::PretendardRegular | Self::PretendardJpRegular => 400,
            Self::PretendardSemiBold => 600,
        }
    }
    pub fn sha256(self) -> &'static str {
        match self {
            Self::PretendardRegular => {
                "6d0af5258997aec7354a6e340fc2325ba321c410ca48b3af858c8c3d6e92a324"
            }
            Self::PretendardSemiBold => {
                "5e1c548732af70873103066c16e1369b9a8a871f0b38c321a1d5bc73e43cea2d"
            }
            Self::PretendardJpRegular => {
                "b14698ed6d82737f45e918d079bca6fc498a107bcb094b56c5229dbddf3246d3"
            }
        }
    }
    pub fn length(self) -> u64 {
        match self {
            Self::PretendardRegular => 2_725_828,
            Self::PretendardSemiBold => 2_671_468,
            Self::PretendardJpRegular => 5_462_268,
        }
    }
    pub fn license(self) -> FontLicense {
        match self {
            Self::PretendardRegular | Self::PretendardSemiBold => FontLicense::Pretendard,
            Self::PretendardJpRegular => FontLicense::PretendardJp,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontLicense {
    Pretendard,
    PretendardJp,
}
impl FontLicense {
    pub fn path(self) -> &'static str {
        match self {
            Self::Pretendard => "assets/fonts/pretendard/LICENSE.txt",
            Self::PretendardJp => "assets/fonts/pretendard/LICENSE-JP.txt",
        }
    }
    pub fn sha256(self) -> &'static str {
        match self {
            Self::Pretendard => "b04538c9abec39a3db75108cf0af0fd9c77032fe8aa2cf38345b4d250e98e38e",
            Self::PretendardJp => {
                "6cb2adce04090e5e321168d5b47e12d1cb703da64929a4f34dba9162eb55eb98"
            }
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

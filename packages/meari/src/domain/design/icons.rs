//! Portable icon vocabulary. Geometry loading and painting belong to adapters.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IconToken {
    SelectionCheck,
    ToggleThumb,
}
impl IconToken {
    pub const ALL: [Self; 2] = [Self::SelectionCheck, Self::ToggleThumb];
    pub const fn name(self) -> &'static str {
        match self {
            Self::SelectionCheck => "selection-check",
            Self::ToggleThumb => "toggle-thumb",
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|token| token.name() == name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum IconSize {
    Xs,
    #[default]
    Sm,
    Md,
    Lg,
}
impl IconSize {
    pub const fn pixels(self) -> u16 {
        match self {
            Self::Xs => 12,
            Self::Sm => 16,
            Self::Md => 20,
            Self::Lg => 24,
        }
    }
    pub const fn class(self) -> &'static str {
        match self {
            Self::Xs => "size-3",
            Self::Sm => "size-4",
            Self::Md => "size-5",
            Self::Lg => "size-6",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum IconTone {
    #[default]
    Inherit,
    Foreground,
    Muted,
    Primary,
    Destructive,
}
impl IconTone {
    pub const fn class(self) -> &'static str {
        match self {
            Self::Inherit => "text-inherit",
            Self::Foreground => "text-foreground",
            Self::Muted => "text-muted-foreground",
            Self::Primary => "text-primary",
            Self::Destructive => "text-destructive",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum IconStroke {
    Thin,
    #[default]
    Regular,
    Bold,
}
impl IconStroke {
    pub const fn width(self) -> u16 {
        match self {
            Self::Thin => 1,
            Self::Regular => 2,
            Self::Bold => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IconSpec {
    pub token: IconToken,
    pub size: IconSize,
    pub tone: IconTone,
    pub stroke: IconStroke,
}
impl IconSpec {
    pub fn new(token: IconToken) -> Self {
        Self {
            token,
            size: IconSize::default(),
            tone: IconTone::default(),
            stroke: IconStroke::default(),
        }
    }
}

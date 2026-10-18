use super::utilities::{COLORS, Conditions, Property, Rule, StyleValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Theme {
    pub light: BTreeMap<String, [u8; 4]>,
    pub dark: BTreeMap<String, [u8; 4]>,
    #[serde(default = "default_spacing")]
    pub spacing_rem: f32,
    #[serde(default = "default_radius")]
    pub radius_rem: f32,
}

fn default_spacing() -> f32 {
    0.25
}
fn default_radius() -> f32 {
    0.625
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            light: BTreeMap::new(),
            dark: BTreeMap::new(),
            spacing_rem: default_spacing(),
            radius_rem: default_radius(),
        }
    }
}

impl Theme {
    pub fn validate(&self) -> Result<(), String> {
        if !self.spacing_rem.is_finite()
            || self.spacing_rem <= 0.0
            || !self.radius_rem.is_finite()
            || self.radius_rem < 0.0
        {
            return Err("finite positive spacing and nonnegative radius required".into());
        }
        for (mode, colors) in [("light", &self.light), ("dark", &self.dark)] {
            for name in COLORS {
                if !colors.contains_key(*name) {
                    return Err(format!("missing {mode} theme token: {name}"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Environment {
    pub viewport_width: f32,
    pub root_font_px: f32,
    pub dark: bool,
}

impl Environment {
    pub fn validate(self) -> Result<(), String> {
        if !self.viewport_width.is_finite()
            || self.viewport_width < 0.0
            || !self.root_font_px.is_finite()
            || self.root_font_px <= 0.0
        {
            return Err("finite nonnegative viewport and positive root font required".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Interaction {
    pub hover: bool,
    pub focus_visible: bool,
    pub active: bool,
    pub disabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ResolvedValue {
    Px(f32),
    Percent(f32),
    Scalar(f32),
    Color([u8; 4]),
    Keyword(String),
}

pub type ResolvedStyle = BTreeMap<Property, ResolvedValue>;

pub fn matches(c: Conditions, env: Environment, state: Interaction) -> bool {
    env.viewport_width >= f32::from(c.min_width)
        && (!c.dark || env.dark)
        && (!c.hover || state.hover)
        && (!c.focus_visible || state.focus_visible)
        && (!c.active || state.active)
        && (!c.disabled || state.disabled)
}

pub fn resolve(
    rules: &[Rule],
    theme: &Theme,
    env: Environment,
    state: Interaction,
) -> Result<ResolvedStyle, String> {
    env.validate()?;
    theme.validate()?;
    let mut style = BTreeMap::new();
    for rule in rules.iter().filter(|r| matches(r.conditions, env, state)) {
        for (property, value) in &rule.declarations {
            let value = match value {
                StyleValue::Spacing(n) => {
                    ResolvedValue::Px(n * theme.spacing_rem * env.root_font_px)
                }
                StyleValue::RadiusFactor(n) => {
                    ResolvedValue::Px(n * theme.radius_rem * env.root_font_px)
                }
                StyleValue::Rem(n) => ResolvedValue::Px(n * env.root_font_px),
                StyleValue::Px(n) => ResolvedValue::Px(*n),
                StyleValue::Percent(n) => ResolvedValue::Percent(*n),
                StyleValue::Scalar(n) => ResolvedValue::Scalar(*n),
                StyleValue::Keyword(v) => ResolvedValue::Keyword(v.clone()),
                StyleValue::Token(name) => ResolvedValue::Color(
                    *(if env.dark { &theme.dark } else { &theme.light })
                        .get(name)
                        .ok_or_else(|| format!("missing theme token: {name}"))?,
                ),
            };
            if matches!(&value, ResolvedValue::Px(n) | ResolvedValue::Percent(n) | ResolvedValue::Scalar(n) if !n.is_finite())
            {
                return Err("native numeric value is not finite".into());
            }
            style.insert(*property, value);
        }
    }
    Ok(style)
}

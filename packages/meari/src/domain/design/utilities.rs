use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Property {
    Display,
    Direction,
    Align,
    Justify,
    Gap,
    PaddingTop,
    PaddingRight,
    PaddingBottom,
    PaddingLeft,
    Width,
    Height,
    Background,
    Foreground,
    BorderColor,
    BorderWidth,
    RingColor,
    RingWidth,
    Radius,
    FontSize,
    FontWeight,
    Opacity,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum StyleValue {
    Spacing(f32),
    RadiusFactor(f32),
    Rem(f32),
    Px(f32),
    Percent(f32),
    Scalar(f32),
    Token(String),
    Keyword(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conditions {
    pub min_width: u16,
    pub dark: bool,
    pub hover: bool,
    pub focus_visible: bool,
    pub active: bool,
    pub disabled: bool,
}

impl Conditions {
    pub fn contains(self, other: Self) -> bool {
        self.min_width >= other.min_width
            && (!other.dark || self.dark)
            && (!other.hover || self.hover)
            && (!other.focus_visible || self.focus_visible)
            && (!other.active || self.active)
            && (!other.disabled || self.disabled)
    }
    pub fn specificity(self) -> (u16, u8) {
        (
            self.min_width,
            [
                self.dark,
                self.hover,
                self.focus_visible,
                self.active,
                self.disabled,
            ]
            .into_iter()
            .filter(|x| *x)
            .count() as u8,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub source: String,
    pub conditions: Conditions,
    pub declarations: Vec<(Property, StyleValue)>,
}

pub const COLORS: &[&str] = &[
    "background",
    "foreground",
    "card",
    "card-foreground",
    "popover",
    "popover-foreground",
    "primary",
    "primary-foreground",
    "secondary",
    "secondary-foreground",
    "muted",
    "muted-foreground",
    "accent",
    "accent-foreground",
    "destructive",
    "border",
    "input",
    "ring",
    "chart-1",
    "chart-2",
    "chart-3",
    "chart-4",
    "chart-5",
    "sidebar",
    "sidebar-foreground",
    "sidebar-primary",
    "sidebar-primary-foreground",
    "sidebar-accent",
    "sidebar-accent-foreground",
    "sidebar-border",
    "sidebar-ring",
];

fn declarations(utility: &str) -> Option<Vec<(Property, StyleValue)>> {
    use Property::*;
    use StyleValue::*;
    let one = |p, v| Some(vec![(p, v)]);
    match utility {
        "flex" | "inline-flex" | "hidden" => return one(Display, Keyword(utility.into())),
        "flex-row" => return one(Direction, Keyword("row".into())),
        "flex-col" => return one(Direction, Keyword("column".into())),
        "border" => return one(BorderWidth, Px(1.0)),
        "rounded-none" => return one(Radius, Px(0.0)),
        "rounded-sm" => return one(Radius, RadiusFactor(0.6)),
        "rounded-md" => return one(Radius, RadiusFactor(0.8)),
        "rounded-lg" => return one(Radius, RadiusFactor(1.0)),
        "rounded-full" => return one(Radius, Px(9999.0)),
        "w-full" => return one(Width, Percent(100.0)),
        "h-full" => return one(Height, Percent(100.0)),
        _ => {}
    }
    for (prefix, property, values) in [
        ("items-", Align, &["start", "end", "center", "stretch"][..]),
        (
            "justify-",
            Justify,
            &["start", "end", "center", "between", "around", "evenly"][..],
        ),
    ] {
        if let Some(value) = utility.strip_prefix(prefix).filter(|v| values.contains(v)) {
            return one(property, Keyword(value.into()));
        }
    }
    if let Some(size) = utility.strip_prefix("text-") {
        let rem = match size {
            "xs" => Some(0.75),
            "sm" => Some(0.875),
            "base" => Some(1.0),
            "lg" => Some(1.125),
            "xl" => Some(1.25),
            _ => None,
        };
        if let Some(rem) = rem {
            return one(FontSize, Rem(rem));
        }
    }
    if let Some(weight) = utility.strip_prefix("font-") {
        let number = match weight {
            "normal" => Some(400.0),
            "medium" => Some(500.0),
            "semibold" => Some(600.0),
            "bold" => Some(700.0),
            _ => None,
        };
        if let Some(number) = number {
            return one(FontWeight, Scalar(number));
        }
    }
    for (prefix, property) in [
        ("bg-", Background),
        ("text-", Foreground),
        ("border-", BorderColor),
        ("ring-", RingColor),
    ] {
        if let Some(token) = utility.strip_prefix(prefix).filter(|t| COLORS.contains(t)) {
            return one(property, Token(token.into()));
        }
    }
    let (prefix, value) = utility.split_once('-')?;
    // Exact integer spellings, no exponential/negative/ambiguous values.
    let n: u16 = value.parse().ok()?;
    if n.to_string() != value {
        return None;
    }
    if prefix == "opacity" && n <= 100 {
        return one(Opacity, Scalar(f32::from(n) / 100.0));
    }
    if ["border", "ring"].contains(&prefix) && [0, 1, 2, 4, 8].contains(&n) {
        return one(
            if prefix == "border" {
                BorderWidth
            } else {
                RingWidth
            },
            Px(f32::from(n)),
        );
    }
    if n > 96 {
        return None;
    }
    let props: &[Property] = match prefix {
        "p" => &[PaddingTop, PaddingRight, PaddingBottom, PaddingLeft],
        "px" => &[PaddingRight, PaddingLeft],
        "py" => &[PaddingTop, PaddingBottom],
        "pt" => &[PaddingTop],
        "pr" => &[PaddingRight],
        "pb" => &[PaddingBottom],
        "pl" => &[PaddingLeft],
        "gap" => &[Gap],
        "w" => &[Width],
        "h" => &[Height],
        _ => return None,
    };
    Some(props.iter().map(|p| (*p, Spacing(f32::from(n)))).collect())
}

pub fn parse(classes: &str) -> Result<Vec<Rule>, String> {
    let mut rules: Vec<Rule> = Vec::new();
    for class in classes.split_whitespace() {
        let mut parts: Vec<_> = class.split(':').collect();
        let utility = parts.pop().unwrap_or_default();
        let mut conditions = Conditions::default();
        let mut seen = std::collections::BTreeSet::new();
        for variant in parts {
            if !seen.insert(variant) {
                return Err(format!("duplicate variant in {class}"));
            }
            match variant {
                "sm" | "md" | "lg" | "xl" | "2xl" => {
                    if conditions.min_width != 0 {
                        return Err(format!("multiple breakpoints in {class}"));
                    }
                    conditions.min_width = match variant {
                        "sm" => 640,
                        "md" => 768,
                        "lg" => 1024,
                        "xl" => 1280,
                        _ => 1536,
                    };
                }
                "dark" => conditions.dark = true,
                "hover" => conditions.hover = true,
                "focus-visible" => conditions.focus_visible = true,
                "active" => conditions.active = true,
                "disabled" => conditions.disabled = true,
                _ => return Err(format!("native variant not implemented: {class}")),
            }
        }
        let declarations = declarations(utility)
            .ok_or_else(|| format!("native utility not implemented: {class}"))?;
        for previous in &rules {
            for (property, _) in &declarations {
                if previous.declarations.iter().any(|(p, _)| p == property)
                    && (previous.conditions == conditions
                        || (!previous.conditions.contains(conditions)
                            && !conditions.contains(previous.conditions)))
                {
                    return Err(format!(
                        "native cascade requires CSS engine: {} / {class}",
                        previous.source
                    ));
                }
            }
        }
        rules.push(Rule {
            source: class.into(),
            conditions,
            declarations,
        });
    }
    rules.sort_by_key(|rule| rule.conditions.specificity());
    Ok(rules)
}

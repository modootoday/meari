//! Shared, source-owned verification palette; not an upstream visual reference.
use meari::domain::design::{theme::Theme, utilities::COLORS};

pub(in super::super) fn reference() -> Theme {
    let mut theme = Theme::default();
    for name in COLORS {
        let foreground = name.contains("foreground");
        theme.light.insert(
            (*name).into(),
            if foreground {
                [24, 24, 27, 255]
            } else {
                [244, 244, 245, 255]
            },
        );
        theme.dark.insert(
            (*name).into(),
            if foreground {
                [244, 244, 245, 255]
            } else {
                [24, 24, 27, 255]
            },
        );
    }
    for (name, color) in [
        ("background", [255, 255, 255, 255]),
        ("primary", [24, 24, 27, 255]),
        ("primary-foreground", [250, 250, 250, 255]),
        ("destructive", [185, 28, 28, 255]),
        ("muted-foreground", [82, 82, 91, 255]),
    ] {
        theme.light.insert(name.into(), color);
    }
    for (name, color) in [
        ("primary", [250, 250, 250, 255]),
        ("primary-foreground", [24, 24, 27, 255]),
        ("destructive", [248, 113, 113, 255]),
        ("muted-foreground", [161, 161, 170, 255]),
    ] {
        theme.dark.insert(name.into(), color);
    }
    theme
}

#[cfg(test)]
mod tests {
    fn luminance([r, g, b, _]: [u8; 4]) -> f64 {
        let linear = |v: u8| {
            let v = f64::from(v) / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
    }
    #[test]
    fn verification_text_and_error_tokens_have_minimum_contrast_in_both_schemes() {
        let theme = super::reference();
        theme.validate().unwrap();
        for colors in [&theme.light, &theme.dark] {
            for (foreground, background) in [
                ("foreground", "background"),
                ("destructive", "background"),
                ("primary-foreground", "primary"),
                ("primary-foreground", "destructive"),
                ("muted-foreground", "muted"),
            ] {
                let a = luminance(colors[foreground]);
                let b = luminance(colors[background]);
                let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
                assert!(ratio >= 4.5, "{foreground}/{background} contrast {ratio}");
            }
        }
    }
}

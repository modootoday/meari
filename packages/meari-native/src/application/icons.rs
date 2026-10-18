//! The only SVG asset registry. Consumers request semantic tokens, never file paths.
use meari::domain::{
    Node, NodeKind, Value,
    design::icons::{IconSize, IconSpec, IconStroke, IconToken, IconTone},
};
use std::collections::BTreeMap;

fn asset(token: IconToken) -> &'static str {
    match token {
        IconToken::SelectionCheck => include_str!("../../assets/icons/selection-check.svg"),
        IconToken::ToggleThumb => include_str!("../../assets/icons/toggle-thumb.svg"),
    }
}

pub fn asset_revision(token: IconToken) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(asset(token).as_bytes()))
}

pub fn render(spec: IconSpec) -> Result<String, String> {
    if spec.token == IconToken::ToggleThumb && spec.stroke != IconStroke::Regular {
        return Err("filled icon does not accept outline stroke tokens".into());
    }
    Ok(asset(spec.token)
        .replacen(
            "<svg ",
            &format!(
                "<svg data-icon=\"{}\" class=\"{} {}\" ",
                spec.token.name(),
                spec.size.class(),
                spec.tone.class()
            ),
            1,
        )
        .replace("width=\"16\"", &format!("width=\"{}\"", spec.size.pixels()))
        .replace(
            "height=\"16\"",
            &format!("height=\"{}\"", spec.size.pixels()),
        )
        .replace(
            "stroke-width=\"2\"",
            &format!("stroke-width=\"{}\"", spec.stroke.width()),
        ))
}

pub fn from_node(
    node: &Node,
    attrs: &mut BTreeMap<String, String>,
) -> Result<(IconSpec, String), String> {
    let NodeKind::Element {
        attributes,
        children,
        ..
    } = &node.kind
    else {
        unreachable!()
    };
    if !children.is_empty() {
        return Err("Icon does not accept children".into());
    }
    let mut spec = IconSpec::new(IconToken::SelectionCheck);
    let mut named = false;
    let mut decorative = true;
    let mut label = None;
    for attr in attributes {
        let string = || match &attr.value {
            Value::String(s) => Ok(s.as_str()),
            _ => Err(format!("Icon.{} requires a static string", attr.name)),
        };
        match attr.name.as_str() {
            "name" => {
                spec.token = IconToken::parse(string()?).ok_or("unknown Icon token")?;
                named = true;
            }
            "size" => {
                spec.size = match string()? {
                    "xs" => IconSize::Xs,
                    "sm" => IconSize::Sm,
                    "md" => IconSize::Md,
                    "lg" => IconSize::Lg,
                    _ => return Err("unknown Icon size token".into()),
                }
            }
            "tone" => {
                spec.tone = match string()? {
                    "inherit" => IconTone::Inherit,
                    "foreground" => IconTone::Foreground,
                    "muted" => IconTone::Muted,
                    "primary" => IconTone::Primary,
                    "destructive" => IconTone::Destructive,
                    _ => return Err("unknown Icon tone token".into()),
                }
            }
            "stroke" => {
                spec.stroke = match string()? {
                    "thin" => IconStroke::Thin,
                    "regular" => IconStroke::Regular,
                    "bold" => IconStroke::Bold,
                    _ => return Err("unknown Icon stroke token".into()),
                }
            }
            "decorative" => match attr.value {
                Value::Bool(v) => decorative = v,
                _ => return Err("Icon.decorative requires boolean".into()),
            },
            "label" => label = Some(string()?.to_owned()),
            "id" | "className" => {}
            name if name.starts_with("data-") => {}
            _ => return Err(format!("Icon prop is not implemented: {}", attr.name)),
        }
    }
    if !named {
        return Err("Icon requires a semantic name token".into());
    }
    if decorative {
        if label.is_some() {
            return Err("decorative Icon cannot have a label".into());
        }
        attrs.insert("aria-hidden".into(), "true".into());
    } else {
        let label = label
            .filter(|s| !s.trim().is_empty())
            .ok_or("semantic Icon requires a label")?;
        attrs.insert("role".into(), "img".into());
        attrs.insert("aria-label".into(), label);
    }
    attrs.insert("data-icon-token".into(), spec.token.name().into());
    Ok((
        spec,
        format!(
            "inline-flex shrink-0 {} {}",
            spec.size.class(),
            spec.tone.class()
        ),
    ))
}

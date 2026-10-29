//! Source-owned group/field mappings; component providers are not executed.
use crate::domain::widgets;
use meari::domain::Node;
use std::collections::BTreeMap;

pub(super) fn tag(tag: &str) -> Option<&'static str> {
    Some(match tag {
        "FieldSet" => "fieldset",
        "FieldLegend" => "legend",
        "FieldLabel" => "label",
        "FieldDescription" => "p",
        "ButtonGroupSeparator" => "hr",
        "ButtonGroup" | "ButtonGroupText" | "Field" | "FieldGroup" | "FieldContent"
        | "FieldTitle" | "FieldError" => "div",
        _ => return None,
    })
}

pub(super) fn attributes(
    tag: &str,
    node: &Node,
    attrs: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    let slot = match tag {
        "ButtonGroup" => "button-group",
        "ButtonGroupText" => "button-group-text",
        "ButtonGroupSeparator" => "button-group-separator",
        "Field" => "field",
        "FieldSet" => "field-set",
        "FieldLegend" => "field-legend",
        "FieldGroup" => "field-group",
        "FieldContent" => "field-content",
        "FieldLabel" | "FieldTitle" => "field-label",
        "FieldDescription" => "field-description",
        "FieldError" => "field-error",
        _ => return Ok(()),
    };
    attrs.insert("data-slot".into(), slot.into());
    for option in [
        "size",
        "defaultValue",
        "defaultOpen",
        "open",
        "modal",
        "collapsible",
    ] {
        if widgets::attr(node, option).is_some() {
            return Err(format!("{tag}: unsupported option {option}"));
        }
    }
    for name in ["orientation", "variant", "decorative"] {
        if let Some(value) = widgets::attr(node, name) {
            let valid = match name {
                "orientation" => {
                    matches!(tag, "ButtonGroup" | "ButtonGroupSeparator" | "Field")
                        && matches!(value, meari::domain::Value::String(_))
                }
                "variant" => {
                    tag == "FieldLegend" && matches!(value, meari::domain::Value::String(_))
                }
                "decorative" => {
                    tag == "ButtonGroupSeparator" && matches!(value, meari::domain::Value::Bool(_))
                }
                _ => false,
            };
            if !valid {
                return Err(format!("{tag}: unsupported option/type {name}"));
            }
        }
    }
    if tag == "FieldSet" && widgets::attr(node, "disabled").is_some() {
        return Err("disabled FieldSet needs first-legend inheritance contract".into());
    }
    if matches!(tag, "ButtonGroup" | "Field" | "ButtonGroupSeparator") {
        let default = if tag == "ButtonGroup" {
            "horizontal"
        } else {
            "vertical"
        };
        let orientation = widgets::string(node, "orientation").unwrap_or(default.into());
        if !["horizontal", "vertical"].contains(&orientation.as_str())
            && !(tag == "Field" && orientation == "responsive")
        {
            return Err(format!("{tag}: invalid orientation"));
        }
        if widgets::attr(node, "orientation").is_some()
            && widgets::string(node, "orientation").is_none()
        {
            return Err(format!("{tag}: orientation requires string"));
        }
        attrs.insert("data-orientation".into(), orientation.clone());
        if tag == "ButtonGroupSeparator" {
            let decorative = widgets::attr(node, "decorative")
                .is_none_or(|v| v == &meari::domain::Value::Bool(true));
            if decorative {
                attrs.insert("aria-hidden".into(), "true".into());
                attrs.insert("role".into(), "presentation".into());
            } else {
                attrs.insert("role".into(), "separator".into());
                attrs.insert("aria-orientation".into(), orientation);
            }
        } else {
            attrs.insert("role".into(), "group".into());
        }
    }
    if tag == "FieldLegend" {
        let variant = widgets::string(node, "variant").unwrap_or("legend".into());
        if !["legend", "label"].contains(&variant.as_str()) {
            return Err("FieldLegend: invalid variant".into());
        }
        attrs.insert("data-variant".into(), variant);
    }
    if tag == "FieldError" {
        attrs.insert("role".into(), "alert".into());
        if matches!(&node.kind,meari::domain::NodeKind::Element{children,..} if children.is_empty())
        {
            attrs.insert("hidden".into(), String::new());
            attrs.insert("aria-hidden".into(), "true".into());
        }
    }
    Ok(())
}

pub(super) fn recipe(tag: &str, node: &Node) -> &'static str {
    match tag {
        "ButtonGroup" if widgets::string(node, "orientation").as_deref() == Some("vertical") => {
            "group/button-group flex w-fit flex-col items-stretch [&>*:not(:first-child)]:rounded-t-none [&>*:not(:first-child)]:border-t-0 [&>*:not(:last-child)]:rounded-b-none"
        }
        "ButtonGroup" => {
            "group/button-group flex w-fit items-stretch [&>*:not(:first-child)]:rounded-l-none [&>*:not(:first-child)]:border-l-0 [&>*:not(:last-child)]:rounded-r-none"
        }
        "ButtonGroupText" => "flex items-center px-3 text-sm",
        "ButtonGroupSeparator"
            if widgets::string(node, "orientation").as_deref() == Some("horizontal") =>
        {
            "self-stretch h-px w-auto border-0 bg-border"
        }
        "ButtonGroupSeparator" => "self-stretch w-px h-auto border-0 bg-border",
        "FieldGroup" => "@container/field-group flex w-full flex-col gap-2",
        "FieldSet" | "FieldContent" => "flex w-full flex-col gap-2",
        "Field" if widgets::string(node, "orientation").as_deref() == Some("horizontal") => {
            "group/field flex w-full flex-row items-center gap-3"
        }
        "Field" if widgets::string(node, "orientation").as_deref() == Some("responsive") => {
            "group/field flex w-full flex-col gap-3 @md/field-group:flex-row"
        }
        "Field" => "group/field flex w-full flex-col gap-2",
        "FieldLabel" | "FieldTitle" => "flex w-fit items-center text-sm font-medium",
        "FieldLegend" => "font-medium",
        "FieldDescription" => "text-sm text-muted-foreground",
        "FieldError" => "text-sm text-destructive",
        _ => "",
    }
}

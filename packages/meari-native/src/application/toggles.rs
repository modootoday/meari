//! Uncontrolled two-state controls. Controlled callbacks and tri-state values remain explicit errors.
use meari::domain::{Node, NodeKind, Value};
use std::collections::BTreeMap;

pub fn icon(tag: &str) -> meari::domain::design::icons::IconSpec {
    use meari::domain::design::icons::{IconSpec, IconToken};
    IconSpec::new(if tag == "Switch" {
        IconToken::ToggleThumb
    } else {
        IconToken::SelectionCheck
    })
}

pub fn attributes(
    tag: &str,
    node: &Node,
    attrs: &mut BTreeMap<String, String>,
) -> Result<Option<bool>, String> {
    if !matches!(tag, "Checkbox" | "Switch") {
        return Ok(None);
    }
    let NodeKind::Element {
        attributes,
        children,
        ..
    } = &node.kind
    else {
        unreachable!()
    };
    if !children.is_empty() {
        return Err(format!("{tag} cannot contain children"));
    }
    let mut checked = false;
    for attribute in attributes {
        match attribute.name.as_str() {
            "checked" => {
                return Err(
                    "controlled checked requires a state binding; use defaultChecked".into(),
                );
            }
            "defaultChecked" => match attribute.value {
                Value::Bool(value) => checked = value,
                _ => return Err("defaultChecked requires boolean".into()),
            },
            "defaultValue" | "open" | "defaultOpen" | "value" | "name" | "required"
            | "readOnly" | "variant" | "size" => {
                return Err(format!("unsupported {tag} state prop"));
            }
            _ => {}
        }
    }
    attrs.insert(
        "role".into(),
        if tag == "Checkbox" {
            "checkbox"
        } else {
            "switch"
        }
        .into(),
    );
    attrs.insert("aria-checked".into(), checked.to_string());
    attrs.insert(
        "data-state".into(),
        if checked { "checked" } else { "unchecked" }.into(),
    );
    attrs.insert("data-toggle".into(), tag.into());
    Ok(Some(checked))
}

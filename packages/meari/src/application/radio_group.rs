//! Lower bound radio semantics to a portable DOM plan without executing callbacks.
use crate::domain::{
    Attribute, Component, Node, NodeKind, Value,
    radio_group::{Direction, Item, Orientation, RadioGroup},
};
#[cfg(any(test, all(target_arch = "wasm32", feature = "wasm")))]
pub(crate) mod plan;

pub(crate) const PREFIX: &str = "data-meari-radio-";
pub(crate) const OWNER: &str = "data-meari-radio-owner";
pub(crate) const CHECKED: &str = "data-meari-radio-selected";
pub(crate) const CONTROLLED: &str = "data-meari-radio-controlled";
pub(crate) const ORIENTATION: &str = "data-meari-radio-orientation";
pub(crate) const DIRECTION: &str = "data-meari-radio-direction";
pub(crate) const LOOP: &str = "data-meari-radio-loop";
pub(crate) const ITEM: &str = "data-meari-radio-value";

#[cfg(test)]
mod direction_tests;

fn attr<'a>(node: &'a Node, name: &str) -> Option<&'a Value> {
    let NodeKind::Element { attributes, .. } = &node.kind else {
        return None;
    };
    attributes
        .iter()
        .find(|attribute| attribute.name == name)
        .map(|attribute| &attribute.value)
}
fn string(node: &Node, name: &str) -> Result<Option<String>, String> {
    match attr(node, name) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(format!("RadioGroup.{name} requires a bound string")),
    }
}
fn boolean(node: &Node, name: &str, default: bool) -> Result<bool, String> {
    match attr(node, name) {
        None => Ok(default),
        Some(Value::Bool(value)) => Ok(*value),
        _ => Err(format!("RadioGroup.{name} requires a bound boolean")),
    }
}

fn model(node: &Node, path: &str, inherited: Option<Direction>) -> Result<RadioGroup, String> {
    let disabled = boolean(node, "disabled", false)?
        || attr(node, "aria-disabled") == Some(&Value::String("true".into()))
        || attr(node, "aria-disabled") == Some(&Value::Bool(true));
    fn collect(
        node: &Node,
        path: &str,
        root: &str,
        disabled: bool,
        items: &mut Vec<Item>,
    ) -> Result<(), String> {
        if let NodeKind::Element { tag, children, .. } = &node.kind {
            if path != root && tag == "RadioGroup" {
                return Ok(());
            }
            if tag == "RadioGroupItem" {
                let value =
                    string(node, "value")?.ok_or("RadioGroupItem requires its declared value")?;
                items.push(Item {
                    path: path.into(),
                    value,
                    disabled: disabled
                        || boolean(node, "disabled", false)?
                        || attr(node, "aria-disabled") == Some(&Value::String("true".into()))
                        || attr(node, "aria-disabled") == Some(&Value::Bool(true)),
                });
            }
            for (index, child) in children.iter().enumerate() {
                collect(child, &format!("{path}/{index}"), root, disabled, items)?;
            }
        }
        Ok(())
    }
    let mut items = Vec::new();
    collect(node, path, path, disabled, &mut items)?;
    let orientation = match string(node, "orientation")?.as_deref() {
        None => Orientation::Unspecified,
        Some("horizontal") => Orientation::Horizontal,
        Some("vertical") => Orientation::Vertical,
        _ => return Err("RadioGroup.orientation requires horizontal or vertical".into()),
    };
    let direction = match string(node, "dir")?.as_deref() {
        None => {
            inherited.ok_or("RadioGroup inherited dir=auto requires host direction resolution")?
        }
        Some(value) if value.eq_ignore_ascii_case("ltr") => Direction::LeftToRight,
        Some(value) if value.eq_ignore_ascii_case("rtl") => Direction::RightToLeft,
        _ => return Err("RadioGroup.dir requires ltr or rtl".into()),
    };
    let checked = string(node, "value")?
        .or(string(node, "defaultValue")?)
        .unwrap_or_default();
    let group = RadioGroup {
        items,
        checked,
        disabled,
        orientation,
        direction,
        looping: boolean(node, "loop", true)?,
    };
    group.validate()?;
    Ok(group)
}

pub(crate) fn prepare(component: &Component) -> Result<Component, String> {
    struct Owner {
        identity: String,
        controlled: bool,
        model: RadioGroup,
    }
    fn set(attributes: &mut Vec<Attribute>, name: &str, value: String) {
        attributes.retain(|attribute| attribute.name != name);
        attributes.push(Attribute {
            name: name.into(),
            value: Value::String(value),
        });
    }
    fn visit(
        node: &mut Node,
        path: &str,
        identity: &str,
        owner: Option<&Owner>,
        inherited: Option<Direction>,
    ) -> Result<(), String> {
        let NodeKind::Element {
            tag, attributes, ..
        } = &node.kind
        else {
            return Ok(());
        };
        if attributes
            .iter()
            .any(|attribute| attribute.name.starts_with(PREFIX))
        {
            return Err("radio DOM metadata is reserved".into());
        }
        let identity = match attr(node, crate::application::repeat::IDENTITY_ATTRIBUTE) {
            Some(Value::String(key)) => format!("key-{key}"),
            _ => identity.to_owned(),
        };
        let direction = Direction::inherit_html(
            match attr(node, "dir") {
                Some(Value::String(value)) => Some(value.as_str()),
                _ => None,
            },
            inherited,
        );
        let group = if tag == "RadioGroup" {
            Some(Owner {
                identity: identity.clone(),
                controlled: attr(node, "value").is_some(),
                model: model(node, path, direction)?,
            })
        } else {
            None
        };
        let owner = group.as_ref().or(owner);
        let original = tag.clone();
        let required = if original == "RadioGroup" {
            boolean(node, "required", false)?
        } else {
            false
        };
        if ["RadioGroup", "RadioGroupItem"].contains(&original.as_str())
            && boolean(node, "asChild", false)?
        {
            return Err("radio asChild requires composed event admission".into());
        }
        let NodeKind::Element {
            tag,
            attributes,
            children,
        } = &mut node.kind
        else {
            unreachable!()
        };
        if original == "RadioGroup" || original == "RadioGroupItem" {
            if attributes
                .iter()
                .any(|attribute| attribute.name.starts_with("on"))
            {
                return Err("radio callback requires its scoped source projection".into());
            }
            let owner = owner.ok_or("RadioGroupItem requires a RadioGroup root")?;
            let custom = attributes
                .iter()
                .find(|attribute| attribute.name == "className")
                .map(|attribute| &attribute.value);
            let custom = match custom {
                None => String::new(),
                Some(Value::String(value)) => value.clone(),
                _ => return Err("radio className requires a bound string".into()),
            };
            let recipe = crate::domain::design::components::recipe(&original, "default", "default");
            set(
                attributes,
                "className",
                format!("{recipe} {custom}").trim().to_owned(),
            );
            attributes.retain(|attribute| {
                ![
                    "asChild",
                    "value",
                    "defaultValue",
                    "orientation",
                    "loop",
                    "disabled",
                    "required",
                ]
                .contains(&attribute.name.as_str())
            });
            if original == "RadioGroup" {
                *tag = "div".into();
                set(attributes, "role", "radiogroup".into());
                set(attributes, OWNER, owner.identity.clone());
                set(attributes, CHECKED, owner.model.checked.clone());
                set(attributes, CONTROLLED, owner.controlled.to_string());
                set(attributes, LOOP, owner.model.looping.to_string());
                set(
                    attributes,
                    DIRECTION,
                    if owner.model.direction == Direction::RightToLeft {
                        "rtl"
                    } else {
                        "ltr"
                    }
                    .into(),
                );
                let orientation = match owner.model.orientation {
                    Orientation::Unspecified => "both",
                    Orientation::Horizontal => "horizontal",
                    Orientation::Vertical => "vertical",
                };
                set(attributes, ORIENTATION, orientation.into());
                if orientation != "both" {
                    set(attributes, "aria-orientation", orientation.into());
                }
                if owner.model.disabled {
                    set(attributes, "aria-disabled", "true".into());
                }
                if required {
                    set(attributes, "aria-required", "true".into());
                }
            } else {
                *tag = "button".into();
                let item = owner
                    .model
                    .items
                    .iter()
                    .find(|item| item.path == path)
                    .ok_or("radio item is outside its owner")?;
                set(attributes, "type", "button".into());
                set(attributes, "role", "radio".into());
                set(attributes, ITEM, item.value.clone());
                let checked = !owner.model.checked.is_empty() && owner.model.checked == item.value;
                set(attributes, "aria-checked", checked.to_string());
                set(
                    attributes,
                    "data-state",
                    if checked { "checked" } else { "unchecked" }.into(),
                );
                set(
                    attributes,
                    "tabIndex",
                    if owner.model.tab_stop(None)? == Some(path) {
                        "0"
                    } else {
                        "-1"
                    }
                    .into(),
                );
                if item.disabled {
                    attributes.push(Attribute {
                        name: "disabled".into(),
                        value: Value::Bool(true),
                    });
                    set(attributes, "aria-disabled", "true".into());
                }
            }
        }
        for (index, child) in children.iter_mut().enumerate() {
            visit(
                child,
                &format!("{path}/{index}"),
                &format!("{identity}/{index}"),
                owner,
                direction,
            )?;
        }
        Ok(())
    }
    let mut prepared = component.clone();
    visit(
        &mut prepared.root,
        "0",
        "0",
        None,
        Some(Direction::LeftToRight),
    )?;
    Ok(prepared)
}

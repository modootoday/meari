//! Accepted frame radio state; never derive selection authority from unowned DOM.
use super::*;
use crate::application::dom::DomNode;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub(crate) struct Group {
    pub root: String,
    pub identity: String,
    pub controlled: bool,
    pub model: RadioGroup,
    pub active: Option<String>,
}

pub(crate) fn groups(plan: &DomNode) -> Result<BTreeMap<String, Group>, String> {
    fn visit(
        node: &DomNode,
        path: &str,
        owner: Option<&str>,
        groups: &mut BTreeMap<String, Group>,
    ) -> Result<(), String> {
        let DomNode::Element {
            attributes,
            children,
            ..
        } = node
        else {
            return Ok(());
        };
        let owner = if let Some(identity) = attributes.get(OWNER) {
            let orientation = match attributes.get(ORIENTATION).map(String::as_str) {
                Some("horizontal") => Orientation::Horizontal,
                Some("vertical") => Orientation::Vertical,
                Some("both") => Orientation::Unspecified,
                _ => return Err("radio plan orientation missing".into()),
            };
            let flag = |name| match attributes.get(name).map(String::as_str) {
                Some("true") => Ok(true),
                Some("false") => Ok(false),
                _ => Err(format!("invalid radio plan flag: {name}")),
            };
            let direction = match attributes.get(DIRECTION).map(String::as_str) {
                Some("rtl") => Direction::RightToLeft,
                Some("ltr") => Direction::LeftToRight,
                _ => return Err("radio plan direction missing".into()),
            };
            groups.insert(
                path.into(),
                Group {
                    root: path.into(),
                    identity: identity.clone(),
                    controlled: flag(CONTROLLED)?,
                    model: RadioGroup {
                        items: Vec::new(),
                        checked: attributes
                            .get(CHECKED)
                            .ok_or("radio checked plan missing")?
                            .clone(),
                        disabled: attributes.get("aria-disabled").map(String::as_str)
                            == Some("true"),
                        orientation,
                        direction,
                        looping: flag(LOOP)?,
                    },
                    active: None,
                },
            );
            Some(path)
        } else {
            owner
        };
        if let Some(value) = attributes.get(ITEM) {
            let group = owner
                .and_then(|owner| groups.get_mut(owner))
                .ok_or("radio item plan has no owner")?;
            let disabled = attributes.contains_key("disabled")
                || attributes.get("aria-disabled").map(String::as_str) == Some("true");
            group.model.items.push(Item {
                path: path.into(),
                value: value.clone(),
                disabled,
            });
            if attributes.get("tabindex").map(String::as_str) == Some("0")
                && group.active.replace(path.into()).is_some()
            {
                return Err("radio plan has multiple tab stops".into());
            }
        }
        for (index, child) in children.iter().enumerate() {
            visit(child, &format!("{path}/{index}"), owner, groups)?;
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    visit(plan, "0", None, &mut result)?;
    let mut identities = BTreeSet::new();
    for group in result.values() {
        group.model.validate()?;
        if !identities.insert(&group.identity) {
            return Err("duplicate radio plan owner".into());
        }
    }
    Ok(result)
}

pub(crate) fn set_state(
    plan: &mut DomNode,
    root: &str,
    checked: &str,
    active: Option<&str>,
) -> Result<(), String> {
    fn visit(
        node: &mut DomNode,
        path: &str,
        root: &str,
        checked: &str,
        active: Option<&str>,
    ) -> Result<(), String> {
        let DomNode::Element {
            attributes,
            children,
            ..
        } = node
        else {
            return Err("radio path crosses text".into());
        };
        if path != root && attributes.contains_key(OWNER) {
            return Ok(());
        }
        if path == root {
            attributes.insert(CHECKED.into(), checked.into());
        }
        if let Some(value) = attributes.get(ITEM) {
            let selected = !checked.is_empty() && value == checked;
            let disabled = attributes.contains_key("disabled")
                || attributes.get("aria-disabled").map(String::as_str) == Some("true");
            attributes.insert("aria-checked".into(), selected.to_string());
            attributes.insert(
                "data-state".into(),
                if selected { "checked" } else { "unchecked" }.into(),
            );
            attributes.insert(
                "tabindex".into(),
                if !disabled && active == Some(path) {
                    "0"
                } else {
                    "-1"
                }
                .into(),
            );
        }
        for (index, child) in children.iter_mut().enumerate() {
            if matches!(child, DomNode::Element { .. }) {
                visit(child, &format!("{path}/{index}"), root, checked, active)?;
            }
        }
        Ok(())
    }
    let mut node = plan;
    for index in root.split('/').skip(1) {
        let index: usize = index.parse().map_err(|_| "invalid radio owner path")?;
        let DomNode::Element { children, .. } = node else {
            return Err("radio owner path crosses text".into());
        };
        node = children.get_mut(index).ok_or("radio owner disappeared")?;
    }
    visit(node, root, root, checked, active)
}

/// Prop patches retain uncontrolled state; event patches explicitly bypass retention.
pub(crate) fn retain(
    previous: &DomNode,
    next: &mut DomNode,
    focused: Option<&str>,
) -> Result<(), String> {
    let previous = groups(previous)?
        .into_values()
        .map(|group| (group.identity.clone(), group))
        .collect::<BTreeMap<_, _>>();
    for mut group in groups(next)?.into_values() {
        let Some(before) = previous.get(&group.identity) else {
            continue;
        };
        if before.controlled != group.controlled {
            return Err("controlled radio mode changed; remount is required".into());
        }
        if !group.controlled {
            group.model.checked = before.model.checked.clone();
        }
        let focused_value = focused
            .and_then(|path| before.model.items.iter().find(|item| item.path == path))
            .map(|item| item.value.as_str());
        let active = focused_value
            .and_then(|value| {
                group
                    .model
                    .items
                    .iter()
                    .find(|item| !item.disabled && item.value == value)
            })
            .map(|item| item.path.clone());
        let active = match active {
            Some(path) => Some(path),
            None => group.model.tab_stop(None)?.map(str::to_owned),
        };
        set_state(next, &group.root, &group.model.checked, active.as_deref())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{Language, binding, dom},
        runtime::IntoValue,
    };

    fn prepared(value: &str, controlled: bool, items: Vec<&str>) -> DomNode {
        let source = format!(
            "import {{RadioGroup,RadioGroupItem}} from '@/components/ui/radio-group';type Props={{checked:string;items:string[]}};export function Choices(props:Props){{return <RadioGroup {}={{props.checked}}>{{props.items.map(item=><RadioGroupItem key={{item}} value={{item}} aria-label={{item}}/>)}}</RadioGroup>;}}",
            if controlled { "value" } else { "defaultValue" }
        );
        let component = crate::parse(&source, Language::Tsx).unwrap();
        let bound = binding::bind(
            &component,
            &BTreeMap::from([
                ("checked".into(), Value::String(value.into())),
                ("items".into(), items.into_value()),
            ]),
        )
        .unwrap();
        dom::prepare(&bound).unwrap()
    }

    #[test]
    fn official_label_composition_lowers_for_and_rejects_unimplemented_as_child() {
        let source = "import {Label} from '@/components/ui/label';import {RadioGroup,RadioGroupItem} from '@/components/ui/radio-group';type Props={};export function Labelled(props:Props){return <RadioGroup aria-label=\"Choices\"><div><RadioGroupItem value=\"a\" id=\"r-a\"/><Label htmlFor=\"r-a\">A</Label></div></RadioGroup>;}";
        let component = crate::parse(source, Language::Tsx).unwrap();
        let frame = dom::prepare(&component).unwrap();
        let DomNode::Element { children, .. } = frame else {
            panic!()
        };
        let DomNode::Element { children, .. } = &children[0] else {
            panic!()
        };
        let DomNode::Element {
            tag, attributes, ..
        } = &children[1]
        else {
            panic!()
        };
        assert_eq!(tag, "label");
        assert_eq!(attributes["for"], "r-a");
        let unsupported = crate::parse(
            &source.replace("value=\"a\"", "asChild value=\"a\""),
            Language::Tsx,
        )
        .unwrap();
        assert!(dom::prepare(&unsupported).is_err());
    }

    #[test]
    fn portable_none_plan_has_one_tab_stop_without_selecting_the_first_item() {
        let frame = prepared("", true, vec!["a", "b"]);
        let groups = groups(&frame).unwrap();
        let group = &groups["0"];
        assert!(group.controlled);
        assert_eq!(group.model.checked, "");
        assert_eq!(group.active.as_deref(), Some("0/0"));
        let DomNode::Element {
            attributes,
            children,
            ..
        } = frame
        else {
            panic!()
        };
        assert_eq!(attributes["role"], "radiogroup");
        for child in children {
            let DomNode::Element {
                tag, attributes, ..
            } = child
            else {
                panic!()
            };
            assert_eq!(tag, "button");
            assert_eq!(attributes["role"], "radio");
            assert_eq!(attributes["aria-checked"], "false");
        }
    }

    #[test]
    fn uncontrolled_event_selection_survives_default_prop_and_keyed_order_changes() {
        let mut before = prepared("", false, vec!["a", "b"]);
        set_state(&mut before, "0", "b", Some("0/1")).unwrap();
        let mut after = prepared("a", false, vec!["b", "new", "a"]);
        retain(&before, &mut after, Some("0/1")).unwrap();
        let group = groups(&after).unwrap().remove("0").unwrap();
        assert_eq!(group.model.checked, "b");
        assert_eq!(group.active.as_deref(), Some("0/0"));
        assert!(retain(&before, &mut prepared("a", true, vec!["a", "b"]), None).is_err());
    }

    #[test]
    fn controlled_prop_commit_updates_checked_and_retains_independent_roving_focus() {
        let before = prepared("a", true, vec!["a", "b"]);
        let mut after = prepared("b", true, vec!["b", "a"]);
        retain(&before, &mut after, Some("0/0")).unwrap();
        let group = groups(&after).unwrap().remove("0").unwrap();
        assert_eq!(group.model.checked, "b");
        assert_eq!(group.active.as_deref(), Some("0/1"));
        let mut unfocused = prepared("b", true, vec!["b", "a"]);
        retain(&before, &mut unfocused, None).unwrap();
        assert_eq!(
            groups(&unfocused).unwrap()["0"].active.as_deref(),
            Some("0/0")
        );
    }

    #[test]
    fn nested_owners_are_isolated_and_source_cannot_inject_radio_metadata() {
        let source = "import {RadioGroup,RadioGroupItem} from '@/components/ui/radio-group';type Props={};export function Nested(props:Props){return <div><RadioGroup defaultValue=\"a\"><RadioGroupItem value=\"a\"/><RadioGroupItem value=\"b\"/><RadioGroup defaultValue=\"x\"><RadioGroupItem value=\"x\"/></RadioGroup></RadioGroup><RadioGroup defaultValue=\"z\"><RadioGroupItem value=\"z\"/></RadioGroup></div>;}";
        let component = crate::parse(source, Language::Tsx).unwrap();
        let mut frame = dom::prepare(&component).unwrap();
        set_state(&mut frame, "0/0", "b", Some("0/0/1")).unwrap();
        let groups = groups(&frame).unwrap();
        assert_eq!(groups.len(), 3);
        assert_eq!(groups["0/0"].model.checked, "b");
        assert_eq!(groups["0/0/2"].model.checked, "x");
        assert_eq!(groups["0/1"].model.checked, "z");
        assert!(
            crate::parse(
                &source.replace("defaultValue=\"a\"", "data-meari-radio-owner=\"forged\""),
                Language::Tsx
            )
            .is_err()
        );
    }
}

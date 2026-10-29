//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use meari::domain::{Node, NodeKind, Value};

pub fn attr<'a>(node: &'a Node, name: &str) -> Option<&'a Value> {
    match &node.kind {
        NodeKind::Element { attributes, .. } => {
            attributes.iter().find(|a| a.name == name).map(|a| &a.value)
        }
        _ => None,
    }
}

pub fn string(node: &Node, name: &str) -> Option<String> {
    match attr(node, name) {
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    }
}

pub fn boolean(node: &Node, name: &str) -> bool {
    attr(node, name) == Some(&Value::Bool(true))
}

pub fn family(tag: &str) -> Option<Family> {
    Some(match tag {
        "Tabs" => Family::Tabs,
        "RadioGroup" => Family::RadioGroup,
        "Accordion" => Family::Accordion,
        "Dialog" => Family::Dialog,
        "AlertDialog" => Family::AlertDialog,
        "Popover" => Family::Popover,
        "Select" => Family::Select,
        "DropdownMenu" => Family::Menu,
        _ => return None,
    })
}

impl Session {
    pub fn is_control(&self, path: &str) -> bool {
        self.owner(path).is_some_and(|w| {
            w.trigger.as_deref() == Some(path)
                || w.close_paths.iter().any(|p| p == path)
                || w.items.iter().any(|item| item.path == path)
        })
    }

    pub fn focus_item(&mut self, path: &str) -> Result<bool, String> {
        let Some(root) = self.owner(path).map(|w| w.root.clone()) else {
            return Ok(false);
        };
        let widget = self.widgets.get_mut(&root).unwrap();
        let Some(item) = widget.items.iter().find(|i| i.path == path) else {
            return Ok(false);
        };
        if item.disabled {
            return Err("cannot focus disabled composite item".into());
        }
        widget.active = Some(path.into());
        self.focus = Some(path.into());
        if widget.family == Family::Tabs {
            widget.value = item.value.clone();
        }
        self.changes.push(Change {
            root,
            value: widget.value.clone(),
            open: widget.open,
        });
        Ok(true)
    }

    pub fn top_overlay(&self) -> Option<&Widget> {
        let is_overlay = |w: &&Widget| {
            w.open && ![Family::Tabs, Family::RadioGroup, Family::Accordion].contains(&w.family)
        };
        self.changes
            .iter()
            .rev()
            .filter_map(|change| self.widgets.get(&change.root))
            .find(is_overlay)
            .or_else(|| self.widgets.values().rev().find(is_overlay))
    }

    pub fn top_modal(&self) -> Option<&Widget> {
        let is_modal = |widget: &&Widget| {
            widget.open
                && widget.modal
                && [Family::Dialog, Family::AlertDialog].contains(&widget.family)
        };
        self.changes
            .iter()
            .rev()
            .filter_map(|change| self.widgets.get(&change.root))
            .find(is_modal)
            .or_else(|| self.widgets.values().rev().find(is_modal))
    }

    pub fn from_tree(root: &Node) -> Result<Self, String> {
        fn visit(
            node: &Node,
            path: String,
            owner: Option<String>,
            item_value: Option<String>,
            item_disabled: bool,
            inherited: Option<meari::domain::radio_group::Direction>,
            session: &mut Session,
        ) -> Result<(), String> {
            let NodeKind::Element { tag, children, .. } = &node.kind else {
                return Ok(());
            };
            let direction = meari::domain::radio_group::Direction::inherit_html(
                string(node, "dir").as_deref(),
                inherited,
            );
            let item_disabled = if let Some(family) = family(tag) {
                [Family::Accordion, Family::RadioGroup].contains(&family)
                    && (boolean(node, "disabled")
                        || (family == Family::RadioGroup
                            && (string(node, "aria-disabled").as_deref() == Some("true")
                                || boolean(node, "aria-disabled"))))
            } else {
                item_disabled || (tag == "AccordionItem" && boolean(node, "disabled"))
            };
            let composed_disabled = boolean(node, "asChild")
                && children.first().is_some_and(|child| {
                    boolean(child, "disabled")
                        || string(child, "aria-disabled").as_deref() == Some("true")
                });
            let disabled = composed_disabled
                || boolean(node, "disabled")
                || string(node, "aria-disabled").as_deref() == Some("true")
                || (["AccordionTrigger", "RadioGroupItem"].contains(&tag.as_str())
                    && item_disabled);
            if disabled {
                session.disabled_paths.insert(path.clone());
            }
            let expected = if family(tag).is_none() {
                match meari::domain::design::components::module(tag) {
                    Some("tabs") => Some(Family::Tabs),
                    Some("radio-group") => Some(Family::RadioGroup),
                    Some("accordion") => Some(Family::Accordion),
                    Some("dialog") => Some(Family::Dialog),
                    Some("alert-dialog") => Some(Family::AlertDialog),
                    Some("popover") => Some(Family::Popover),
                    Some("select") => Some(Family::Select),
                    Some("dropdown-menu") => Some(Family::Menu),
                    _ => None,
                }
            } else {
                None
            };
            if let Some(expected) = expected
                && owner
                    .as_ref()
                    .and_then(|owner| session.widgets.get(owner))
                    .map(|w| w.family)
                    != Some(expected)
            {
                return Err(format!("{tag}: missing or incorrect composite root"));
            }
            let owner = if let Some(family) = family(tag) {
                if family == Family::Accordion
                    && string(node, "type").is_some_and(|kind| kind != "single")
                {
                    return Err("multiple-selection Accordion remains unimplemented".into());
                }
                for name in ["value", "defaultValue"] {
                    if attr(node, name).is_some_and(|v| !matches!(v, Value::String(_))) {
                        return Err(format!("{tag}.{name} requires bound string"));
                    }
                }
                for name in ["open", "defaultOpen"] {
                    if attr(node, name).is_some_and(|v| !matches!(v, Value::Bool(_))) {
                        return Err(format!("{tag}.{name} requires bound bool"));
                    }
                }
                let dialog = [Family::Dialog, Family::AlertDialog].contains(&family);
                if dialog
                    && attr(node, "modal").is_some_and(|value| !matches!(value, Value::Bool(_)))
                {
                    return Err(format!("{tag}.modal requires bound bool"));
                }
                let modal = dialog
                    && attr(node, "modal").is_none_or(|value| matches!(value, Value::Bool(true)));
                if family == Family::AlertDialog && !modal {
                    return Err("nonmodal AlertDialog is unsupported".into());
                }
                use meari::domain::radio_group::{Direction, Orientation};
                let mut radio_orientation = Orientation::Unspecified;
                let mut radio_direction = Direction::LeftToRight;
                if family == Family::RadioGroup {
                    for name in ["disabled", "required"] {
                        if attr(node, name).is_some_and(|value| !matches!(value, Value::Bool(_))) {
                            return Err(format!("RadioGroup.{name} requires a bound boolean"));
                        }
                    }
                    radio_orientation = match attr(node, "orientation") {
                        None => Orientation::Unspecified,
                        Some(Value::String(value)) if value == "vertical" => Orientation::Vertical,
                        Some(Value::String(value)) if value == "horizontal" => {
                            Orientation::Horizontal
                        }
                        _ => {
                            return Err(
                                "RadioGroup.orientation requires horizontal or vertical".into()
                            );
                        }
                    };
                    radio_direction = match attr(node, "dir") {
                        None => direction.ok_or(
                            "RadioGroup inherited dir=auto requires host direction resolution",
                        )?,
                        Some(Value::String(value)) if value.eq_ignore_ascii_case("ltr") => {
                            Direction::LeftToRight
                        }
                        Some(Value::String(value)) if value.eq_ignore_ascii_case("rtl") => {
                            Direction::RightToLeft
                        }
                        _ => return Err("RadioGroup.dir requires ltr or rtl".into()),
                    };
                    if attr(node, "loop").is_some_and(|value| !matches!(value, Value::Bool(_))) {
                        return Err("RadioGroup.loop requires a bound boolean".into());
                    }
                }
                session.widgets.insert(
                    path.clone(),
                    Widget {
                        root: path.clone(),
                        family,
                        items: Vec::new(),
                        value: string(node, "value")
                            .or_else(|| string(node, "defaultValue"))
                            .unwrap_or_default(),
                        active: None,
                        open: attr(node, "open").map_or_else(
                            || boolean(node, "defaultOpen"),
                            |value| matches!(value, Value::Bool(true)),
                        ),
                        controlled_open: dialog && attr(node, "open").is_some(),
                        controlled_value: family == Family::RadioGroup
                            && attr(node, "value").is_some(),
                        radio_orientation,
                        radio_direction,
                        radio_loop: attr(node, "loop")
                            .is_none_or(|value| matches!(value, Value::Bool(true))),
                        modal,
                        trigger: None,
                        content: None,
                        close_paths: Vec::new(),
                        vertical: string(node, "orientation")
                            .map_or(family != Family::Tabs, |v| v == "vertical"),
                    },
                );
                Some(path.clone())
            } else {
                owner
            };
            if let Some(owner) = &owner {
                let widget = session.widgets.get_mut(owner).unwrap();
                if [
                    "TabsTrigger",
                    "RadioGroupItem",
                    "AccordionTrigger",
                    "SelectItem",
                    "DropdownMenuItem",
                ]
                .contains(&tag.as_str())
                {
                    let value = string(node, "value")
                        .or_else(|| item_value.clone())
                        .unwrap_or_else(|| path.clone());
                    if tag == "RadioGroupItem"
                        && string(node, "value").is_none_or(|value| value.is_empty())
                    {
                        return Err("RadioGroupItem requires a nonempty bound string value".into());
                    }
                    if tag == "RadioGroupItem"
                        && attr(node, "disabled")
                            .is_some_and(|value| !matches!(value, Value::Bool(_)))
                    {
                        return Err("RadioGroupItem.disabled requires a bound boolean".into());
                    }
                    if widget.items.iter().any(|i| i.value == value) {
                        return Err(format!("duplicate widget item value: {value}"));
                    }
                    widget.items.push(Item {
                        path: path.clone(),
                        value,
                        disabled,
                    });
                }
                if tag.ends_with("Trigger")
                    && !["TabsTrigger", "AccordionTrigger"].contains(&tag.as_str())
                {
                    widget.trigger = Some(path.clone());
                }
                if tag.ends_with("Content")
                    && !["TabsContent", "AccordionContent"].contains(&tag.as_str())
                {
                    widget.content = Some(path.clone());
                }
                if tag.ends_with("Close")
                    || tag == "AlertDialogCancel"
                    || tag == "AlertDialogAction"
                {
                    widget.close_paths.push(path.clone());
                }
            }
            let item_value = if tag == "AccordionItem" {
                string(node, "value")
            } else {
                item_value
            };
            for (index, child) in children.iter().enumerate() {
                visit(
                    child,
                    format!("{path}/{index}"),
                    owner.clone(),
                    item_value.clone(),
                    item_disabled,
                    direction,
                    session,
                )?;
            }
            Ok(())
        }
        let mut session = Self::default();
        visit(
            root,
            "0".into(),
            None,
            None,
            false,
            Some(meari::domain::radio_group::Direction::LeftToRight),
            &mut session,
        )?;
        for widget in session.widgets.values_mut() {
            if widget.family == Family::RadioGroup {
                widget.radio_model(false).validate()?;
            }
            if widget.family == Family::Tabs && widget.value.is_empty() {
                widget.value = widget
                    .items
                    .iter()
                    .find(|i| !i.disabled)
                    .map(|i| i.value.clone())
                    .unwrap_or_default();
            }
            widget.active = widget
                .items
                .iter()
                .find(|i| !i.disabled && i.value == widget.value)
                .or_else(|| widget.items.iter().find(|i| !i.disabled))
                .map(|i| i.path.clone());
            if [
                Family::Dialog,
                Family::AlertDialog,
                Family::Popover,
                Family::Select,
                Family::Menu,
            ]
            .contains(&widget.family)
                && (widget.trigger.is_none() || widget.content.is_none())
            {
                return Err(format!(
                    "{:?} requires a trigger and content",
                    widget.family
                ));
            }
        }
        Ok(session)
    }

    pub fn owner(&self, path: &str) -> Option<&Widget> {
        self.widgets
            .values()
            .filter(|w| path == w.root || path.starts_with(&format!("{}/", w.root)))
            .max_by_key(|w| w.root.len())
    }
}

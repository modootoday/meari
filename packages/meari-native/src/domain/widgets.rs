//! Renderer-neutral composite widget policy. Actual OS event/a11y adapters are separate.
use meari::domain::{Node, NodeKind, Value};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Family {
    Tabs,
    Accordion,
    Dialog,
    AlertDialog,
    Popover,
    Select,
    Menu,
}

#[derive(Clone, Debug, Serialize)]
pub struct Item {
    pub path: String,
    pub value: String,
    pub disabled: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Widget {
    pub root: String,
    pub family: Family,
    pub items: Vec<Item>,
    pub value: String,
    pub active: Option<String>,
    pub open: bool,
    pub trigger: Option<String>,
    pub content: Option<String>,
    pub close_paths: Vec<String>,
    pub vertical: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Session {
    pub widgets: BTreeMap<String, Widget>,
    pub focus: Option<String>,
    pub changes: Vec<Change>,
    pub disabled_paths: BTreeSet<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Change {
    pub root: String,
    pub value: String,
    pub open: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Activate {
        path: String,
    },
    Key {
        path: String,
        key: String,
        shift: bool,
    },
    DismissOutside {
        path: String,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum FocusMove {
    Next,
    Previous,
    First,
    Last,
}

pub trait Navigation {
    fn move_focus(
        &self,
        items: &[Item],
        active: Option<&str>,
        movement: FocusMove,
    ) -> Option<String>;
}

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
        let is_overlay =
            |w: &&Widget| w.open && ![Family::Tabs, Family::Accordion].contains(&w.family);
        self.changes
            .iter()
            .rev()
            .filter_map(|change| self.widgets.get(&change.root))
            .find(is_overlay)
            .or_else(|| self.widgets.values().rev().find(is_overlay))
    }
    pub fn from_tree(root: &Node) -> Result<Self, String> {
        fn visit(
            node: &Node,
            path: String,
            owner: Option<String>,
            item_value: Option<String>,
            item_disabled: bool,
            session: &mut Session,
        ) -> Result<(), String> {
            let NodeKind::Element { tag, children, .. } = &node.kind else {
                return Ok(());
            };
            let item_disabled = if let Some(family) = family(tag) {
                family == Family::Accordion && boolean(node, "disabled")
            } else {
                item_disabled || (tag == "AccordionItem" && boolean(node, "disabled"))
            };
            let disabled = boolean(node, "disabled")
                || string(node, "aria-disabled").as_deref() == Some("true")
                || (tag == "AccordionTrigger" && item_disabled);
            if disabled {
                session.disabled_paths.insert(path.clone());
            }
            let expected = if family(tag).is_none() {
                match meari::domain::design::components::module(tag) {
                    Some("tabs") => Some(Family::Tabs),
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
                        open: boolean(node, "open") || boolean(node, "defaultOpen"),
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
                    "AccordionTrigger",
                    "SelectItem",
                    "DropdownMenuItem",
                ]
                .contains(&tag.as_str())
                {
                    let value = string(node, "value")
                        .or_else(|| item_value.clone())
                        .unwrap_or_else(|| path.clone());
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
                    session,
                )?;
            }
            Ok(())
        }
        let mut session = Self::default();
        visit(root, "0".into(), None, None, false, &mut session)?;
        for widget in session.widgets.values_mut() {
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

    /// The runtime supplies visible focusable descendants for modal Tab trapping.
    pub fn dispatch(
        &mut self,
        action: Action,
        navigation: &dyn Navigation,
        focusable: &[String],
    ) -> Result<bool, String> {
        if let Action::DismissOutside { path } = &action {
            let Some(overlay) = self.top_overlay() else {
                return Ok(false);
            };
            let inside = overlay
                .content
                .as_ref()
                .into_iter()
                .chain(overlay.trigger.as_ref())
                .any(|scope| path == scope || path.starts_with(&format!("{scope}/")));
            if inside || overlay.family == Family::AlertDialog {
                return Ok(false);
            }
            let root = overlay.root.clone();
            let overlay = self.widgets.get_mut(&root).unwrap();
            overlay.open = false;
            self.focus = overlay.trigger.clone();
            self.changes.push(Change {
                root,
                value: overlay.value.clone(),
                open: false,
            });
            return Ok(true);
        }
        let (path, key, shift) = match action {
            Action::Activate { path } => (path, None, false),
            Action::Key { path, key, shift } => (path, Some(key), shift),
            Action::DismissOutside { .. } => unreachable!(),
        };
        if self.disabled_paths.contains(&path) {
            return Ok(false);
        }
        let Some(owner) = self.owner(&path).map(|w| w.root.clone()) else {
            return Ok(false);
        };
        let widget = self.widgets.get_mut(&owner).unwrap();
        let before = (
            widget.value.clone(),
            widget.open,
            widget.active.clone(),
            self.focus.clone(),
        );
        let item = widget.items.iter().find(|i| path == i.path).cloned();
        if item.as_ref().is_some_and(|i| i.disabled) {
            return Ok(false);
        }
        if key.as_deref() == Some("Escape") && widget.open && widget.family != Family::AlertDialog {
            widget.open = false;
            self.focus = widget.trigger.clone();
        } else if key.as_deref() == Some("Tab")
            && widget.open
            && [Family::Dialog, Family::AlertDialog].contains(&widget.family)
        {
            if focusable.is_empty() {
                self.focus = widget.content.clone();
            } else {
                let position = focusable.iter().position(|p| p == &path);
                let index = if shift {
                    position.map_or(focusable.len() - 1, |i| {
                        (i + focusable.len() - 1) % focusable.len()
                    })
                } else {
                    position.map_or(0, |i| (i + 1) % focusable.len())
                };
                self.focus = Some(focusable[index].clone());
            }
        } else if let Some(movement) = match key.as_deref() {
            Some("ArrowDown") if widget.vertical => Some(FocusMove::Next),
            Some("ArrowUp") if widget.vertical => Some(FocusMove::Previous),
            Some("ArrowRight") if !widget.vertical => Some(FocusMove::Next),
            Some("ArrowLeft") if !widget.vertical => Some(FocusMove::Previous),
            Some("Home") => Some(FocusMove::First),
            Some("End") => Some(FocusMove::Last),
            _ => None,
        } {
            if let Some(active) =
                navigation.move_focus(&widget.items, widget.active.as_deref(), movement)
            {
                widget.active = Some(active.clone());
                self.focus = Some(active.clone());
                if widget.family == Family::Tabs {
                    widget.value = widget
                        .items
                        .iter()
                        .find(|i| i.path == active)
                        .unwrap()
                        .value
                        .clone();
                } else if [Family::Select, Family::Menu].contains(&widget.family) {
                    widget.open = true;
                }
            }
        } else if key.is_none() || matches!(key.as_deref(), Some("Enter" | " ")) {
            if widget.trigger.as_deref() == Some(&path) {
                widget.open = !widget.open;
                self.focus = if widget.open {
                    widget
                        .active
                        .clone()
                        .or_else(|| focusable.first().cloned())
                        .or_else(|| widget.content.clone())
                } else {
                    widget.trigger.clone()
                };
            } else if let Some(item) = item {
                widget.active = Some(item.path.clone());
                self.focus = Some(item.path);
                if widget.family == Family::Accordion && widget.value == item.value {
                    widget.value.clear();
                } else {
                    widget.value = item.value;
                }
                if [Family::Select, Family::Menu].contains(&widget.family) {
                    widget.open = false;
                    self.focus = widget.trigger.clone();
                }
            } else if widget.close_paths.contains(&path) {
                widget.open = false;
                self.focus = widget.trigger.clone();
            }
        }
        let changed = before
            != (
                widget.value.clone(),
                widget.open,
                widget.active.clone(),
                self.focus.clone(),
            );
        if changed {
            self.changes.push(Change {
                root: owner,
                value: widget.value.clone(),
                open: widget.open,
            });
        }
        Ok(changed)
    }
}

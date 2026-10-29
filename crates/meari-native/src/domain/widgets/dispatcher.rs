//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;

impl Session {
    /// The runtime supplies visible focusable descendants for modal Tab trapping.
    pub fn dispatch(
        &mut self,
        action: Action,
        navigation: &dyn Navigation,
        focusable: &[String],
    ) -> Result<bool, String> {
        if self.requested_open(&action).is_some() {
            return Err("controlled open requires a scoped Boolean intent dispatcher".into());
        }
        let radio_path = match &action {
            Action::Activate { path }
            | Action::Key { path, .. }
            | Action::DismissOutside { path } => path,
        };
        if self
            .owner(radio_path)
            .is_some_and(|widget| widget.family == Family::RadioGroup)
        {
            return Err("radio selection requires its owned renderer adapter".into());
        }
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
            && widget.modal
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

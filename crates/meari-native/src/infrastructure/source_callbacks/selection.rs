use super::InteractiveDocument;
use crate::domain::widgets::{Action, Family};
use meari::domain::host_callbacks::CallbackArguments;

impl InteractiveDocument {
    pub(in crate::infrastructure) fn sync_radio_focus(&mut self) -> Result<(), String> {
        if !self
            .session
            .widgets
            .values()
            .any(|widget| widget.family == Family::RadioGroup)
        {
            return Ok(());
        }
        let focused = self
            .renderer
            .document
            .get_focussed_node_id()
            .and_then(|id| {
                self.elements()
                    .iter()
                    .find(|element| {
                        self.renderer.document.get_element_by_id(&element.id) == Some(id)
                    })
                    .map(|element| element.path.clone())
            });
        let previous = self.session.clone();
        let paths = self
            .elements()
            .iter()
            .map(|element| (element.path.clone(), element.path.clone()))
            .collect();
        let focus_plan = self.prepare_controlled_focus(&previous, &paths);
        let mut changed = false;
        for (path, widget) in &mut self.session.widgets {
            if widget.family != Family::RadioGroup {
                continue;
            }
            let active = widget
                .radio_model(self.session.disabled_paths.contains(path))
                .tab_stop(focused.as_deref())?
                .map(str::to_owned);
            changed |= active != widget.active;
            widget.active = active;
        }
        if !changed {
            return Ok(());
        }
        self.session.focus = focused;
        if let Err(error) = self.patch() {
            self.session = previous;
            if let Err(recovery) = self
                .patch()
                .and_then(|()| self.restore_controlled_focus(&focus_plan))
            {
                self.source_input_poisoned = true;
                return Err(format!(
                    "{error}; radio tab-stop recovery failed: {recovery}"
                ));
            }
            return Err(error);
        }
        Ok(())
    }

    fn admit_selection_item(
        &self,
        owner: &str,
        target: &str,
        callback: bool,
    ) -> Result<(), String> {
        if callback {
            self.admit_callback_owner(owner)?;
        }
        let id = self
            .path_id(target)
            .and_then(|id| self.renderer.document.get_element_by_id(id))
            .ok_or("radio selection target identity missing")?;
        if callback
            && self
                .callbacks
                .as_ref()
                .and_then(|callbacks| callbacks.targets.get(target))
                != Some(&id)
        {
            return Err("radio selection target was replaced".into());
        }
        self.admit_callback_target(owner, None, false)?;
        self.admit_callback_target(target, Some("button"), false)?;
        let element = self
            .elements()
            .iter()
            .find(|element| element.path == target)
            .ok_or("radio selection target is not owned")?;
        if element.attributes.get("role").map(String::as_str) != Some("radio") {
            return Err("selection target is not an accepted radio item".into());
        }
        Ok(())
    }

    pub(in crate::infrastructure) fn dispatch_selection(
        &mut self,
        action: &Action,
    ) -> Result<Option<bool>, String> {
        let path = match action {
            Action::Activate { path } | Action::Key { path, .. } => path,
            Action::DismissOutside { .. } => return Ok(None),
        };
        let Some(widget) = self
            .session
            .owner(path)
            .filter(|widget| widget.family == Family::RadioGroup)
            .cloned()
        else {
            return Ok(None);
        };
        let Some(target) = widget
            .items
            .iter()
            .filter(|item| path == &item.path || path.starts_with(&format!("{}/", item.path)))
            .max_by_key(|item| item.path.len())
            .map(|item| item.path.clone())
        else {
            return Ok(None);
        };
        let callback = self
            .callbacks
            .as_ref()
            .is_some_and(|callbacks| callbacks.is_selection(&widget.root));
        let mut model = widget.radio_model(self.session.disabled_paths.contains(&widget.root));
        // Hidden or unavailable items cannot become keyboard destinations.
        for item in &mut model.items {
            item.disabled |= self
                .admit_callback_target(&item.path, Some("button"), false)
                .is_err();
        }
        let request = match action {
            Action::Activate { .. } => model.activate(&target)?,
            Action::Key { key, .. } => model.key(&target, key)?,
            Action::DismissOutside { .. } => unreachable!(),
        };
        let Some(request) = request else {
            return Ok(Some(false));
        };
        self.admit_selection_item(&widget.root, &target, callback)?;
        self.admit_selection_item(&widget.root, &request.focus, callback)?;
        let previous = self.session.clone();
        let old_paths = self
            .elements()
            .iter()
            .map(|element| (element.path.clone(), element.path.clone()))
            .collect();
        let focus_plan = self.prepare_controlled_focus(&previous, &old_paths);
        let current = self
            .session
            .widgets
            .get_mut(&widget.root)
            .ok_or("radio root disappeared")?;
        current.active = Some(request.focus.clone());
        if !current.controlled_value {
            current.value = request.value.clone();
        }
        self.session.focus = Some(request.focus.clone());
        let publish = self.patch().and_then(|()| {
            let id = self
                .path_id(&request.focus)
                .and_then(|id| self.renderer.document.get_element_by_id(id))
                .ok_or("radio focus target disappeared after patch")?;
            self.renderer.document.set_focus_to(id);
            if self.renderer.document.get_focussed_node_id() != Some(id) {
                return Err("radio focus patch was not accepted".into());
            }
            self.renderer.refresh_styles();
            self.admit_selection_item(&widget.root, &request.focus, callback)?;
            if callback && request.changes_checked {
                self.callbacks
                    .as_mut()
                    .ok_or("radio callback dispatcher missing")?
                    .enqueue_arguments_guarded(
                        &widget.root,
                        CallbackArguments::Text(request.value.clone()),
                    )?;
            }
            Ok(())
        });
        if let Err(error) = publish {
            self.session = previous;
            if let Err(recovery) = self
                .patch()
                .and_then(|()| self.restore_controlled_focus(&focus_plan))
            {
                self.source_input_poisoned = true;
                return Err(format!("{error}; radio focus recovery failed: {recovery}"));
            }
            self.renderer.refresh_styles();
            return Err(error);
        }
        self.session.changes.push(crate::domain::widgets::Change {
            root: widget.root.clone(),
            value: self.session.widgets[&widget.root].value.clone(),
            open: false,
        });
        Ok(Some(true))
    }
}

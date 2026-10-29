use super::InteractiveDocument;
use crate::domain::widgets::Action;
use meari::domain::host_callbacks::CallbackArguments;
#[cfg(test)]
mod tests;

impl InteractiveDocument {
    fn enqueue_boolean_intent(
        &mut self,
        owner: &str,
        target: &str,
        value: bool,
        toggle: bool,
    ) -> Result<(), String> {
        if !self
            .callbacks
            .as_ref()
            .is_some_and(|callbacks| callbacks.is_boolean(owner))
        {
            return Err("Boolean callback path is not registered".into());
        }
        self.admit_callback_owner(owner)?;
        let current_target = self
            .path_id(target)
            .and_then(|id| self.renderer.document.get_element_by_id(id));
        if current_target.is_none()
            || self
                .callbacks
                .as_ref()
                .and_then(|callbacks| callbacks.targets.get(target))
                .copied()
                != current_target
        {
            return Err("Boolean callback action target was replaced".into());
        }
        self.admit_callback_target(owner, None, false)?;
        self.admit_callback_target(target, toggle.then_some("button"), toggle)?;
        self.callbacks
            .as_mut()
            .ok_or("Boolean callback dispatcher missing")?
            .enqueue_arguments_guarded(owner, CallbackArguments::Boolean(value))
    }

    /// Submit an explicit value from a trusted host without committing controlled state.
    pub fn enqueue_callback_boolean(&mut self, path: &str, value: bool) -> Result<(), String> {
        let element = self
            .elements()
            .iter()
            .find(|element| element.path == path)
            .ok_or("Boolean callback target missing")?;
        if !element.attributes.contains_key("data-toggle")
            || !element
                .attributes
                .contains_key("data-meari-controlled-toggle")
        {
            return Err("explicit Boolean input requires a two-state toggle".into());
        }
        self.enqueue_boolean_intent(path, path, value, true)
    }

    pub(in crate::infrastructure) fn dispatch_boolean_callback(
        &mut self,
        action: &Action,
    ) -> Result<Option<bool>, String> {
        let path = match action {
            Action::Activate { path }
            | Action::Key { path, .. }
            | Action::DismissOutside { path } => path,
        };
        let owner = if matches!(action, Action::DismissOutside { .. }) {
            self.session.top_overlay()
        } else {
            self.session.owner(path)
        };
        if let Some(widget) = owner
            && !widget.controlled_open
            && self
                .callbacks
                .as_ref()
                .is_some_and(|callbacks| callbacks.is_boolean(&widget.root))
        {
            return Err("Boolean open callbacks require explicit controlled open".into());
        }
        if let Some(request) = self.session.requested_open(action) {
            self.enqueue_boolean_intent(&request.root, &request.target, request.open, false)?;
            return Ok(Some(true));
        }
        let target = match action {
            Action::Activate { path } => path,
            Action::Key { path, key, .. } if key == " " => path,
            _ => return Ok(None),
        };
        let registered = self
            .callbacks
            .as_ref()
            .is_some_and(|callbacks| callbacks.is_boolean(target));
        if !registered {
            return Ok(None);
        }
        let element = self
            .elements()
            .iter()
            .find(|element| element.path == *target)
            .ok_or("Boolean toggle target missing")?;
        if !element.attributes.contains_key("data-toggle")
            || !element
                .attributes
                .contains_key("data-meari-controlled-toggle")
        {
            return Err("Boolean activation requires a two-state toggle".into());
        }
        let checked = match element.attributes.get("aria-checked").map(String::as_str) {
            Some("true") => true,
            Some("false") => false,
            _ => return Err("Boolean toggle has no authoritative checked value".into()),
        };
        self.enqueue_boolean_intent(target, target, !checked, true)?;
        Ok(Some(true))
    }
}

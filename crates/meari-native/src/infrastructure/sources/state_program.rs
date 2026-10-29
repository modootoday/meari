use super::super::document::InteractiveDocument;
use crate::domain::widgets::{Action, Session};

pub(in super::super) fn mount(
    source: meari::domain::Component,
) -> Result<meari::application::state_program::StateProgram, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static EPOCH: AtomicU64 = AtomicU64::new(1);
    let epoch = EPOCH
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| "native state epoch exhausted")?;
    meari::application::state_program::StateProgram::mount(
        source,
        "native-source-root".into(),
        epoch,
    )
}

impl InteractiveDocument {
    /// Dispatch a typed host payload through the current source definition.
    pub fn dispatch_source_input(
        &mut self,
        path: &str,
        payload: &meari::domain::input_event::InputPayload,
    ) -> Result<(), String> {
        if self.source_input_poisoned {
            return Err("source input document requires remount after failed recovery".into());
        }
        let binding = self
            .state_inputs
            .get(path)
            .ok_or("source input handler missing")?
            .clone();
        let target = self
            .elements()
            .iter()
            .find(|element| element.path == path)
            .ok_or("source input target missing")?;
        if target.attributes.contains_key("disabled")
            || target.attributes.contains_key("readonly")
            || self.elements().iter().any(|element| {
                (element.path == path || path.starts_with(&format!("{}/", element.path)))
                    && (element.attributes.contains_key("hidden")
                        || element.attributes.contains_key("inert")
                        || element
                            .attributes
                            .get("aria-disabled")
                            .is_some_and(|value| value == "true"))
            })
        {
            return Err("source input is disabled, readOnly, hidden or inert".into());
        }
        if self.dispatch_module_input(path, payload)? {
            return Ok(());
        }
        let mut program = self
            .state_program
            .as_ref()
            .ok_or("source state runtime missing")?
            .clone();
        let handler = program.input_handler(binding)?;
        let projection = program.dispatch_input(&handler, payload)?;
        self.commit_state_projection(program, projection)
    }

    pub fn has_state_event(&self, path: &str) -> bool {
        self.state_events.contains_key(path)
            || self.module_program.as_ref().is_some_and(|program| {
                program.project().is_ok_and(|projection| {
                    projection.event_paths().any(|candidate| candidate == path)
                })
            })
    }
    pub fn component_state(
        &self,
    ) -> Result<meari::application::component_state::StateCommit, String> {
        self.state_program
            .as_ref()
            .ok_or("document has no source state")?
            .snapshot()
    }

    pub(in super::super) fn dispatch_state(
        &mut self,
        action: &Action,
    ) -> Result<Option<bool>, String> {
        if self.source_input_poisoned {
            return Err("source input document requires remount after failed recovery".into());
        }
        let path = match action {
            Action::Activate { path } => path,
            Action::Key { path, key, .. } if key == "Enter" || key == " " => path,
            _ => return Ok(None),
        };
        let Some(event) = self.state_events.get(path).cloned() else {
            return Ok(None);
        };
        let element = self
            .elements()
            .iter()
            .find(|element| element.path == *path)
            .ok_or("state event target missing")?;
        if element.attributes.contains_key("disabled")
            || self.elements().iter().any(|element| {
                (element.path == *path || path.starts_with(&format!("{}/", element.path)))
                    && element.attributes.contains_key("hidden")
            })
        {
            return Ok(Some(false));
        }
        let mut program = self
            .state_program
            .as_ref()
            .ok_or("source state runtime missing")?
            .clone();
        let projection = program.dispatch(&event)?;
        self.commit_state_projection(program, projection)?;
        Ok(Some(true))
    }

    pub(in super::super) fn commit_state_projection(
        &mut self,
        program: meari::application::state_program::StateProgram,
        projection: meari::application::state_program::StateProjection,
    ) -> Result<(), String> {
        self.commit_component_projection(projection.component)?;
        self.state_program = Some(program);
        self.state_events = projection.events;
        self.state_inputs = projection.inputs;
        Ok(())
    }

    pub(in super::super) fn commit_component_projection(
        &mut self,
        component: meari::domain::Component,
    ) -> Result<(), String> {
        if self.source_input_poisoned {
            return Err("source input document requires remount after failed recovery".into());
        }
        let mut next_session = Session::from_tree(&component.root)?;
        crate::application::state_program::retained_shape(&self.source.root, &component.root)?;
        let next_markup = crate::application::markup::build(&component, &next_session)?;
        let old_path = next_markup
            .elements
            .iter()
            .filter_map(|next| {
                self.elements
                    .iter()
                    .find(|old| old.id == next.id)
                    .map(|old| (next.path.clone(), old.path.clone()))
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let radio_focused = self
            .renderer
            .document
            .get_focussed_node_id()
            .and_then(|id| {
                self.elements
                    .iter()
                    .find(|element| {
                        self.renderer.document.get_element_by_id(&element.id) == Some(id)
                    })
                    .map(|element| element.path.clone())
            });
        for (path, next) in &mut next_session.widgets {
            if let Some(old) = old_path
                .get(path)
                .and_then(|path| self.session.widgets.get(path))
            {
                if next.controlled_value != old.controlled_value {
                    return Err("controlled selection mode changed; remount is required".into());
                }
                if !next.controlled_value {
                    next.value = old.value.clone();
                }
                if next.controlled_open != old.controlled_open {
                    return Err("controlled open mode changed; remount is required".into());
                }
                if !next.controlled_open {
                    next.open = old.open;
                }
                if next.family == crate::domain::widgets::Family::RadioGroup {
                    let fallback = next
                        .radio_model(next_session.disabled_paths.contains(path))
                        .tab_stop(None)?
                        .map(str::to_owned);
                    next.active = radio_focused
                        .as_ref()
                        .filter(|path| old.items.iter().any(|item| item.path == **path))
                        .and_then(|active| {
                            old_path
                                .iter()
                                .find(|(_, previous)| *previous == active)
                                .map(|(path, _)| path.clone())
                        })
                        .filter(|path| {
                            next.items
                                .iter()
                                .any(|item| item.path == *path && !item.disabled)
                        })
                        .or(fallback);
                } else {
                    next.active = old.active.clone();
                }
            }
        }
        next_session.focus = self.session.focus.as_ref().and_then(|path| {
            old_path
                .iter()
                .find(|(_, old)| *old == path)
                .map(|(path, _)| path.clone())
        });
        next_session.changes = self.session.changes.clone();
        let focus_plan = self.prepare_controlled_focus(&next_session, &old_path);
        let previous_inputs = std::mem::take(&mut self.input_values);
        let previous_checked = std::mem::take(&mut self.checked_values);
        let previous_files = std::mem::take(&mut self.file_values);
        self.input_values = super::super::keyed_document::remap(
            &previous_inputs,
            &self.elements,
            &next_markup.elements,
        );
        self.checked_values = super::super::keyed_document::remap(
            &previous_checked,
            &self.elements,
            &next_markup.elements,
        );
        self.file_values = super::super::keyed_document::remap(
            &previous_files,
            &self.elements,
            &next_markup.elements,
        );
        let previous_source = std::mem::replace(&mut self.source, component);
        let previous_session = std::mem::replace(&mut self.session, next_session);
        if let Err(error) = self
            .patch()
            .and_then(|()| self.apply_controlled_focus(&focus_plan))
        {
            self.source = previous_source;
            self.session = previous_session;
            self.input_values = previous_inputs;
            self.checked_values = previous_checked;
            self.file_values = previous_files;
            if let Err(recovery) = self
                .patch()
                .and_then(|()| self.restore_controlled_focus(&focus_plan))
            {
                self.source_input_poisoned = true;
                return Err(format!(
                    "{error}; controlled projection recovery failed: {recovery}"
                ));
            }
            return Err(error);
        }
        Ok(())
    }
}

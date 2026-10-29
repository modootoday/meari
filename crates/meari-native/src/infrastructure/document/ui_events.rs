//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use crate::domain::widgets::{Action, Family};
use blitz_dom::{DocGuard, DocGuardMut, Document, EventDriver, EventHandler, NodeId};
use blitz_traits::events::{DomEvent, DomEventData, EventState, UiEvent};

impl EventHandler for QueuedHandler<'_> {
    fn handle_event(
        &mut self,
        chain: &[NodeId],
        event: &mut DomEvent,
        document: &mut dyn Document,
        state: &mut EventState,
    ) {
        let doc = document.inner();
        let paths: Vec<_> = chain
            .iter()
            .filter_map(|id| {
                doc.get_node(*id)?
                    .attr(blitz_dom::LocalName::from("data-bridge-path"))
                    .map(str::to_owned)
            })
            .collect();
        let path = paths.first().cloned();
        let Some(path) = path else {
            return;
        };
        if let DomEventData::Input(input) = &event.data {
            use meari::domain::input_event::{InputKind, InputPayload};
            if self.callback_inputs.contains(&path) {
                self.input_events
                    .push((path.clone(), InputPayload::Text(input.value.clone())));
            } else if let Some(binding) = self.source_inputs.get(&path) {
                let payload = match binding.kind {
                    InputKind::Text => Some(InputPayload::Text(input.value.clone())),
                    InputKind::Checked => chain
                        .first()
                        .and_then(|id| doc.get_node(*id))
                        .and_then(|node| node.element_data())
                        .and_then(|data| data.checkbox_input_checked())
                        .map(InputPayload::Checked),
                };
                if let Some(payload) = payload {
                    self.input_events.push((path.clone(), payload));
                }
            } else {
                self.inputs.insert(path.clone(), input.value.clone());
            }
        }
        if let DomEventData::Click(_) = &event.data
            && let Some(overlay) = self.session.top_overlay()
        {
            let inside = overlay
                .content
                .as_ref()
                .into_iter()
                .chain(overlay.trigger.as_ref())
                .any(|scope| path == *scope || path.starts_with(&format!("{scope}/")));
            if !inside {
                self.actions
                    .push(Action::DismissOutside { path: path.clone() });
                if overlay.modal && [Family::Dialog, Family::AlertDialog].contains(&overlay.family)
                {
                    state.prevent_default();
                    return;
                }
            }
        }
        if let DomEventData::Click(_) = &event.data
            && let Some(button) = chain.iter().find_map(|id| {
                let node = doc.get_node(*id)?;
                ([
                    blitz_dom::local_name!("button"),
                    blitz_dom::local_name!("summary"),
                ]
                .contains(&node.element_data()?.name.local))
                .then(|| {
                    node.attr(blitz_dom::LocalName::from("data-bridge-path"))
                        .map(str::to_owned)
                })
                .flatten()
            })
            && !self.session.is_control(&button)
        {
            self.actions.push(Action::Activate { path: button });
            state.prevent_default();
            return;
        }
        if let DomEventData::KeyDown(key) = &event.data
            && !key.is_composing
            && ["Enter", " "].contains(&key.key.to_string().as_str())
            && !self.session.is_control(&path)
            && chain
                .first()
                .and_then(|id| doc.get_node(*id))
                .and_then(|node| node.element_data())
                .is_some_and(|e| {
                    [
                        blitz_dom::local_name!("button"),
                        blitz_dom::local_name!("summary"),
                    ]
                    .contains(&e.name.local)
                })
        {
            let controlled_toggle = chain.iter().any(|id| {
                doc.get_node(*id).is_some_and(|node| {
                    node.attr(blitz_dom::LocalName::from("data-meari-controlled-toggle"))
                        .is_some()
                })
            });
            self.actions.push(if controlled_toggle {
                Action::Key {
                    path,
                    key: key.key.to_string(),
                    shift: key.modifiers.shift(),
                }
            } else {
                Action::Activate { path }
            });
            state.prevent_default();
            return;
        }
        let Some(widget) = self.session.owner(&path) else {
            return;
        };
        match &event.data {
            DomEventData::Click(_) => {
                if let Some(path) = paths.iter().find(|path| {
                    widget.trigger.as_ref() == Some(path)
                        || widget.close_paths.contains(path)
                        || widget.items.iter().any(|item| item.path == **path)
                }) {
                    self.actions.push(Action::Activate { path: path.clone() });
                    state.prevent_default();
                }
            }
            DomEventData::KeyDown(key) if !key.is_composing => {
                let key_name = key.key.to_string();
                let trap = key_name == "Tab"
                    && widget.open
                    && widget.modal
                    && [Family::Dialog, Family::AlertDialog].contains(&widget.family);
                let navigation = widget.items.iter().any(|i| i.path == path)
                    || widget.trigger.as_deref() == Some(&path);
                let control = if widget.family == Family::RadioGroup {
                    navigation
                        && (key_name == "Enter" || widget.radio_model(false).handles_key(&key_name))
                } else {
                    trap || key_name == "Escape"
                        || (widget.close_paths.contains(&path)
                            && ["Enter", " "].contains(&key_name.as_str()))
                        || (navigation
                            && [
                                "ArrowLeft",
                                "ArrowRight",
                                "ArrowUp",
                                "ArrowDown",
                                "Home",
                                "End",
                                "Enter",
                                " ",
                            ]
                            .contains(&key_name.as_str()))
                };
                if control {
                    self.actions.push(Action::Key {
                        path,
                        key: key_name,
                        shift: key.modifiers.shift(),
                    });
                    state.prevent_default();
                }
            }
            _ => {}
        }
    }
}

impl Document for InteractiveDocument {
    fn inner(&self) -> DocGuard<'_> {
        self.renderer.document.inner()
    }
    fn inner_mut(&mut self) -> DocGuardMut<'_> {
        self.renderer.document.inner_mut()
    }
    fn handle_ui_event(&mut self, event: UiEvent) {
        if self.source_input_poisoned {
            self.last_error =
                Some("source input document requires remount after failed recovery".into());
            return;
        }
        let mut actions = Vec::new();
        let epoch = self
            .callback_scope()
            .map_or(self.application_generation, |scope| scope.source_generation);
        let definition = self
            .state_program
            .as_ref()
            .map_or(self.application_definition_revision, |program| {
                program.definition_revision()
            });
        let commit_owners = self.range_commit_owners();
        if matches!(&event, UiEvent::PointerCancel(_) | UiEvent::PointerLeave(_))
            || matches!(&event, UiEvent::KeyDown(key) if ["Escape", "Tab"].contains(&key.key.to_string().as_str()))
        {
            self.range_inputs.cancel();
        }
        if let Some((path, value)) =
            self.range_inputs
                .pending(&self.renderer.document, epoch, definition, &commit_owners)
        {
            if let Err(error) = self.enqueue_range_commit(&path, &value) {
                self.last_error = Some(error);
                return;
            }
            self.range_inputs.acknowledge();
        }
        match self.range_inputs.handle(
            &mut self.renderer.document,
            epoch,
            definition,
            &commit_owners,
            &event,
        ) {
            Ok(Some(outcome)) => {
                if let Some(value) = outcome.value
                    && let Err(error) = self.set_input_value(&outcome.path, &value)
                {
                    self.range_inputs.cancel();
                    self.last_error = Some(match self.patch() {
                        Ok(()) => error,
                        Err(restore) => {
                            self.source_input_poisoned = true;
                            format!("{error}; range input recovery failed: {restore}")
                        }
                    });
                    return;
                }
                let commit_owners = self.range_commit_owners();
                if let Err(error) = self.range_inputs.finish(
                    &self.renderer.document,
                    epoch,
                    definition,
                    &commit_owners,
                    outcome.release.as_ref(),
                ) {
                    self.range_inputs.cancel();
                    self.last_error = Some(error);
                    return;
                }
                if let Some((path, value)) = self.range_inputs.pending(
                    &self.renderer.document,
                    epoch,
                    definition,
                    &commit_owners,
                ) {
                    if let Err(error) = self.enqueue_range_commit(&path, &value) {
                        self.last_error = Some(error);
                        return;
                    }
                    self.range_inputs.acknowledge();
                }
                return;
            }
            Err(error) => {
                self.range_inputs.cancel();
                self.last_error = Some(error);
                return;
            }
            Ok(None) => {}
        }
        let mut input_events = Vec::new();
        let callback_inputs = self
            .callbacks
            .as_ref()
            .map(|callbacks| callbacks.text_paths())
            .unwrap_or_default();
        let handler = QueuedHandler {
            session: &self.session,
            actions: &mut actions,
            inputs: &mut self.input_values,
            source_inputs: &self.state_inputs,
            input_events: &mut input_events,
            callback_inputs: &callback_inputs,
        };
        let previous_focus = self.renderer.document.get_focussed_node_id();
        EventDriver::new(&mut self.renderer.document, handler).handle_ui_event(event);
        if previous_focus != self.renderer.document.get_focussed_node_id()
            && let Err(error) = self.sync_radio_focus()
        {
            self.last_error = Some(error);
            return;
        }
        for (path, payload) in input_events {
            let result = if callback_inputs.contains(&path) {
                match &payload {
                    meari::domain::input_event::InputPayload::Text(value) => {
                        self.enqueue_callback_text(&path, value)
                    }
                    _ => Err("text callback received a non-text payload".into()),
                }
            } else {
                self.dispatch_source_input(&path, &payload)
            };
            if let Err(error) = result {
                // The engine edits its property before emitting Input; restore the model value.
                self.last_error = Some(match self.patch() {
                    Ok(()) => error,
                    Err(restore) => {
                        self.source_input_poisoned = true;
                        format!("{error}; source input recovery failed: {restore}")
                    }
                });
            }
        }
        for action in actions {
            if let Err(error) = self.dispatch(action) {
                self.last_error = Some(error);
            }
        }
    }
}

//! Resolve events against the current owned DOM, never a captured output index.
use super::{Inner, WebDocument, listener::Listener};
use crate::domain::host_callbacks::CallbackSource;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::{Element, Event, Node};

impl Inner {
    fn current_event_path(
        &self,
        event: &Event,
        input: bool,
    ) -> Result<Option<(String, Element)>, String> {
        if self.closed || self.poisoned {
            return Ok(None);
        }
        let target = event
            .target()
            .and_then(|target| target.dyn_into::<Node>().ok())
            .ok_or("Web event target missing")?;
        if !self.shell.contains(Some(&target)) {
            return Ok(None);
        }
        let mut current = target
            .dyn_ref::<Element>()
            .cloned()
            .or_else(|| target.parent_element());
        while let Some(element) = current {
            if element.is_same_node(Some(self.shell.as_ref())) {
                break;
            }
            if let Some((path, _)) = self
                .frame
                .nodes
                .iter()
                .find(|(_, node)| node.is_same_node(Some(element.as_ref())))
            {
                let callback = self
                    .callbacks
                    .as_ref()
                    .and_then(|callbacks| callbacks.source(path));
                let action = !input
                    && (self.events.contains_key(path)
                        || self.module_program.as_ref().is_some_and(|program| {
                            program.project().is_ok_and(|projection| {
                                projection.event_paths().any(|candidate| candidate == path)
                            })
                        }));
                if matches!(
                    (input, callback),
                    (true, Some(CallbackSource::CurrentTargetText))
                        | (
                            false,
                            Some(CallbackSource::Action | CallbackSource::CapturedText)
                        )
                ) || action
                {
                    return Ok(Some((path.clone(), element)));
                }
            }
            current = element.parent_element();
        }
        Ok(None)
    }
}
impl WebDocument {
    pub(super) fn install_click_dispatcher(&mut self) -> Result<(), String> {
        self.install_event_dispatcher(false)
    }
    pub(super) fn install_input_dispatcher(&mut self) -> Result<(), String> {
        self.install_event_dispatcher(true)
    }
    fn install_event_dispatcher(&mut self, input: bool) -> Result<(), String> {
        let weak = Rc::downgrade(&self.inner);
        let target = self.inner.borrow().shell.clone().unchecked_into();
        self.listeners.push(Listener::capture(
            target,
            if input { "input" } else { "click" },
            move |event| {
                let Some(owner) = weak.upgrade() else {
                    return;
                };
                let result = (|| {
                    let mut inner = owner.borrow_mut();
                    if !input && let Some(path) = inner.radio_event_path(&event)? {
                        event.prevent_default();
                        inner.dispatch_radio(&path, None)?;
                        return Ok(());
                    }
                    let Some((path, current)) = inner.current_event_path(&event, input)? else {
                        return Ok(());
                    };
                    if input {
                        inner.enqueue_callback(
                            &path,
                            &current,
                            Some(&super::source_callbacks::event_value(&current)?),
                        )
                    } else {
                        inner.dispatch(&path).map(|_| ())
                    }
                })();
                if let Err(error) = result {
                    owner.borrow_mut().last_error = Some(error);
                }
            },
        )?);
        Ok(())
    }
}

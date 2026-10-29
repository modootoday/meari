//! Owned composite events produce inert selection requests after focus/frame admission.
use super::super::{Inner, WebDocument, frame::error, listener::Listener};
use crate::{application::radio_group::plan, domain::host_callbacks::CallbackSource};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::{Element, Event, FocusEvent, HtmlElement, KeyboardEvent, Node};

impl Inner {
    pub(in super::super) fn radio_event_path(
        &self,
        event: &Event,
    ) -> Result<Option<String>, String> {
        if self.closed || self.poisoned {
            return Ok(None);
        }
        let target = event
            .target()
            .and_then(|target| target.dyn_into::<Node>().ok())
            .ok_or("radio event target missing")?;
        if !self.shell.contains(Some(&target)) {
            return Ok(None);
        }
        let groups = plan::groups(&self.frame.plan)?;
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
                && groups
                    .values()
                    .any(|group| group.model.items.iter().any(|item| item.path == *path))
            {
                return Ok(Some(path.clone()));
            }
            current = element.parent_element();
        }
        Ok(None)
    }

    fn admit_radio_node(&self, path: &str, role: &str) -> Result<Element, String> {
        let element = self
            .frame
            .nodes
            .get(path)
            .and_then(|node| node.dyn_ref::<Element>())
            .ok_or("radio owned node missing")?;
        if !self.shell.contains(Some(element.as_ref()))
            || element.get_attribute("role").as_deref() != Some(role)
        {
            return Err("radio owned role/identity differs".into());
        }
        if element.matches(":disabled").map_err(error)?
            || element
                .closest("[hidden],[inert],[aria-disabled='true']")
                .map_err(error)?
                .is_some()
        {
            return Err("radio target is disabled, hidden or inert".into());
        }
        let window = element
            .owner_document()
            .and_then(|document| document.default_view())
            .ok_or("radio window missing")?;
        let mut current = Some(element.clone());
        while let Some(node) = current {
            let style = window
                .get_computed_style(&node)
                .map_err(error)?
                .ok_or("radio computed style missing")?;
            if style.get_property_value("display").map_err(error)? == "none"
                || (node.is_same_node(Some(element.as_ref()))
                    && ["hidden", "collapse"].contains(
                        &style
                            .get_property_value("visibility")
                            .map_err(error)?
                            .as_str(),
                    ))
            {
                return Err("radio target is hidden by computed CSS".into());
            }
            current = node.parent_element();
        }
        Ok(element.clone())
    }

    fn restore_radio_focus(&self, previous: Option<&Element>) -> Result<(), String> {
        let document = self
            .shell
            .owner_document()
            .ok_or("radio document missing")?;
        let same = |current: Option<Element>| match (previous, current) {
            (Some(expected), Some(current)) => current.is_same_node(Some(expected.as_ref())),
            (None, None) => true,
            _ => false,
        };
        if same(document.active_element()) {
            return Ok(());
        }
        if let Some(element) = previous.and_then(|element| element.dyn_ref::<HtmlElement>()) {
            element.focus().map_err(error)?;
        }
        if !same(document.active_element())
            && let Some(element) = document
                .active_element()
                .and_then(|element| element.dyn_into::<HtmlElement>().ok())
        {
            element.blur().map_err(error)?;
        }
        if !same(document.active_element()) {
            return Err("radio focus recovery was not accepted".into());
        }
        Ok(())
    }

    pub(in super::super) fn dispatch_radio(
        &mut self,
        path: &str,
        key: Option<&str>,
    ) -> Result<Option<bool>, String> {
        if self.closed || self.poisoned {
            return Err("Web radio document is closed or requires recovery".into());
        }
        let groups = plan::groups(&self.frame.plan)?;
        let Some(mut group) = groups
            .into_values()
            .find(|group| group.model.items.iter().any(|item| item.path == path))
        else {
            return Ok(None);
        };
        self.frame.preflight(&self.frame.plan, &self.shell)?;
        for item in &mut group.model.items {
            item.disabled |= self.admit_radio_node(&item.path, "radio").is_err();
        }
        let request = match key {
            Some(key) => group.model.key(path, key)?,
            None => group.model.activate(path)?,
        };
        let Some(request) = request else {
            return Ok(Some(false));
        };
        self.admit_radio_node(&group.root, "radiogroup")?;
        self.admit_radio_node(path, "radio")?;
        let target = self.admit_radio_node(&request.focus, "radio")?;
        let root = self
            .frame
            .nodes
            .get(&group.root)
            .and_then(|node| node.dyn_ref::<Element>())
            .ok_or("radio root disappeared")?
            .clone();
        let callback = self.callbacks.as_ref().is_some_and(|callbacks| {
            callbacks.source(&group.root) == Some(CallbackSource::SelectionValue)
        });
        let before = self.frame.plan.clone();
        let previous = self
            .shell
            .owner_document()
            .ok_or("radio document missing")?
            .active_element();
        let mut next = before.clone();
        let checked = if group.controlled {
            group.model.checked.as_str()
        } else {
            request.value.as_str()
        };
        plan::set_state(&mut next, &group.root, checked, Some(&request.focus))?;
        let shell = self.shell.clone();
        let result = (|| {
            self.frame
                .patch_interaction(next, &shell)
                .map_err(|(error, poisoned)| {
                    self.poisoned |= poisoned;
                    error
                })?;
            target
                .dyn_ref::<HtmlElement>()
                .ok_or("radio focus target is not HTML")?
                .focus()
                .map_err(error)?;
            if !target
                .owner_document()
                .and_then(|document| document.active_element())
                .is_some_and(|active| active.is_same_node(Some(target.as_ref())))
            {
                return Err("radio focus was not accepted".into());
            }
            self.admit_radio_node(&request.focus, "radio")?;
            if callback && request.changes_checked {
                self.enqueue_callback(&group.root, &root, Some(&request.value))?;
            }
            Ok(())
        })();
        if let Err(original) = result {
            let recovery = self
                .frame
                .patch_interaction(before, &shell)
                .map_err(|(error, _)| error)
                .and_then(|()| self.restore_radio_focus(previous.as_ref()));
            if let Err(recovery) = recovery {
                self.poisoned = true;
                return Err(format!("{original}; radio recovery failed: {recovery}"));
            }
            return Err(original);
        }
        Ok(Some(true))
    }
}

impl WebDocument {
    pub(in super::super) fn install_radio_focus(&mut self) -> Result<(), String> {
        for kind in ["focusin", "focusout"] {
            let weak = Rc::downgrade(&self.inner);
            let target = self.inner.borrow().shell.clone().unchecked_into();
            self.listeners
                .push(Listener::capture(target, kind, move |event| {
                    let Some(owner) = weak.upgrade() else { return };
                    // Programmatic focus during a guarded patch already stages the tab stop.
                    let Ok(mut inner) = owner.try_borrow_mut() else {
                        return;
                    };
                    let result = (|| {
                        let Some(path) = inner.radio_event_path(&event)? else {
                            return Ok(());
                        };
                        let groups = plan::groups(&inner.frame.plan)?;
                        let Some(group) = groups
                            .values()
                            .find(|group| group.model.items.iter().any(|item| item.path == path))
                        else {
                            return Ok(());
                        };
                        let active = if kind == "focusin" {
                            inner.admit_radio_node(&path, "radio")?;
                            Some(path)
                        } else {
                            let related = event
                                .dyn_ref::<FocusEvent>()
                                .and_then(|event| event.related_target())
                                .and_then(|target| target.dyn_into::<Node>().ok());
                            if related.as_ref().is_some_and(|target| {
                                group.model.items.iter().any(|item| {
                                    inner
                                        .frame
                                        .nodes
                                        .get(&item.path)
                                        .is_some_and(|node| node.contains(Some(target)))
                                })
                            }) {
                                return Ok(());
                            }
                            group.model.tab_stop(None)?.map(str::to_owned)
                        };
                        inner.frame.preflight(&inner.frame.plan, &inner.shell)?;
                        let mut next = inner.frame.plan.clone();
                        plan::set_state(
                            &mut next,
                            &group.root,
                            &group.model.checked,
                            active.as_deref(),
                        )?;
                        let shell = inner.shell.clone();
                        inner.frame.patch_interaction(next, &shell).map_err(
                            |(error, poisoned)| {
                                inner.poisoned |= poisoned;
                                error
                            },
                        )?;
                        Ok(())
                    })();
                    if let Err(error) = result {
                        inner.last_error = Some(error);
                    }
                })?);
        }
        Ok(())
    }

    pub(in super::super) fn install_radio_keyboard(&mut self) -> Result<(), String> {
        let weak = Rc::downgrade(&self.inner);
        let target = self.inner.borrow().shell.clone().unchecked_into();
        self.listeners
            .push(Listener::capture(target, "keydown", move |event| {
                let Some(owner) = weak.upgrade() else { return };
                let Some(key) = event.dyn_ref::<KeyboardEvent>() else {
                    return;
                };
                if key.is_composing() {
                    return;
                }
                let result = (|| {
                    let mut inner = owner.borrow_mut();
                    let Some(path) = inner.radio_event_path(&event)? else {
                        return Ok(());
                    };
                    let name = key.key();
                    let handled = plan::groups(&inner.frame.plan)?.values().any(|group| {
                        group.model.items.iter().any(|item| item.path == path)
                            && (name == "Enter" || group.model.handles_key(&name))
                    });
                    if !handled {
                        return Ok(());
                    }
                    event.prevent_default();
                    inner.dispatch_radio(&path, Some(&name))?;
                    Ok(())
                })();
                if let Err(error) = result {
                    owner.borrow_mut().last_error = Some(error);
                }
            })?);
        Ok(())
    }
}

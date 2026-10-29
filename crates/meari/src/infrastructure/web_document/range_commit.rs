//! Range commit is a separate typed event slot, never a captured draft or SDK command.
use super::{Inner, WebDocument, listener::Listener};
use crate::{
    application::host_callbacks::{CallbackEventOwner, DocumentCallbacks},
    domain::{
        plugins::PluginScope,
        range_interaction::{RangeGesture, RangeInteraction, RangeValueCommit},
    },
};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::{Element, Event, HtmlInputElement, KeyboardEvent, PointerEvent};

#[derive(Clone, PartialEq)]
struct Owner {
    path: String,
    node: Element,
    scope: PluginScope,
    definition: u64,
    binding: CallbackEventOwner,
}
#[derive(Default)]
pub(super) struct State {
    interaction: RangeInteraction<Owner>,
    pending: Option<RangeValueCommit<Owner>>,
    pointer_released: bool,
    pointer_changed: bool,
}
impl State {
    fn cancel(&mut self) {
        self.interaction.cancel();
        self.pending = None;
        self.pointer_released = false;
        self.pointer_changed = false;
    }
    fn finish(&mut self, inner: &mut Inner, gesture: &RangeGesture) -> Result<(), String> {
        let Some(owner) = self.interaction.owner().cloned() else {
            return Ok(());
        };
        if !valid(inner, &owner, true) {
            self.cancel();
            return Ok(());
        }
        let actual = super::source_callbacks::event_value(&owner.node)?;
        self.pending = self.interaction.finish(&owner, gesture, &actual)?;
        self.flush(inner)
    }
    pub(super) fn flush(&mut self, inner: &mut Inner) -> Result<(), String> {
        let Some(pending) = self.pending.as_ref() else {
            return Ok(());
        };
        if !valid(inner, &pending.owner, true) {
            self.cancel();
            return Ok(());
        }
        inner.enqueue_callback(
            &DocumentCallbacks::value_commit_key(&pending.owner.path),
            &pending.owner.node,
            Some(&pending.value),
        )?;
        self.pending = None;
        Ok(())
    }
    fn event(&mut self, inner: &mut Inner, event: &Event) -> Result<(), String> {
        let kind = event.type_();
        if inner.closed || inner.poisoned {
            self.cancel();
            return Ok(());
        }
        let target = event
            .target()
            .and_then(|node| node.dyn_into::<Element>().ok());
        let leaves_document = kind == "pointerleave"
            && (target.as_ref() == Some(&inner.shell)
                || event.dyn_ref::<PointerEvent>().is_some_and(|pointer| {
                    pointer
                        .related_target()
                        .and_then(|target| target.dyn_into::<web_sys::Node>().ok())
                        .is_none_or(|node| !inner.shell.contains(Some(&node)))
                }));
        if matches!(kind.as_str(), "pointercancel" | "focusout")
            || leaves_document
            || event.dyn_ref::<KeyboardEvent>().is_some_and(|key| {
                key.type_() == "keydown" && ["Escape", "Tab"].contains(&key.key().as_str())
            })
        {
            self.cancel();
            return Ok(());
        }
        if self
            .interaction
            .owner()
            .is_some_and(|owner| !valid(inner, owner, kind != "pointerdown"))
        {
            self.cancel();
        }
        self.flush(inner)?;
        let current = target;
        match kind.as_str() {
            "pointerdown" => {
                let Some(pointer) = event
                    .dyn_ref::<PointerEvent>()
                    .filter(|pointer| pointer.is_primary() && pointer.button() == 0)
                else {
                    return Ok(());
                };
                let Some(owner) = current.as_ref().and_then(|node| owner(inner, node)) else {
                    self.cancel();
                    return Ok(());
                };
                if !valid(inner, &owner, false) {
                    self.cancel();
                    return Ok(());
                }
                let initial = super::source_callbacks::event_value(&owner.node)?;
                self.interaction.begin(
                    owner,
                    RangeGesture::Pointer(pointer.pointer_id() as u32 as u128),
                    &initial,
                )?;
                self.pointer_released = false;
                self.pointer_changed = false;
            }
            "pointerup" => {
                let Some(pointer) = event.dyn_ref::<PointerEvent>() else {
                    return Ok(());
                };
                let gesture = RangeGesture::Pointer(pointer.pointer_id() as u32 as u128);
                if self.interaction.gesture() != Some(&gesture) {
                    return Ok(());
                }
                if current.as_ref() != self.interaction.owner().map(|owner| &owner.node) {
                    return Ok(());
                }
                self.pointer_released = true;
                let actual = super::source_callbacks::event_value(
                    &self.interaction.owner().expect("matching pointer").node,
                )?;
                if self.pointer_changed || !self.interaction.changed_from_initial(&actual) {
                    self.finish(inner, &gesture)?;
                }
            }
            "change" if matches!(self.interaction.gesture(), Some(RangeGesture::Pointer(_))) => {
                if current.as_ref() != self.interaction.owner().map(|owner| &owner.node) {
                    return Ok(());
                }
                self.pointer_changed = true;
                if self.pointer_released {
                    let gesture = self
                        .interaction
                        .gesture()
                        .cloned()
                        .expect("pointer interaction");
                    self.finish(inner, &gesture)?;
                }
            }
            "keydown" => {
                let Some(key) = event
                    .dyn_ref::<KeyboardEvent>()
                    .filter(|key| !key.is_composing())
                else {
                    return Ok(());
                };
                if ![
                    "ArrowLeft",
                    "ArrowRight",
                    "ArrowUp",
                    "ArrowDown",
                    "Home",
                    "End",
                    "PageUp",
                    "PageDown",
                ]
                .contains(&key.key().as_str())
                {
                    return Ok(());
                }
                let Some(owner) = current.as_ref().and_then(|node| owner(inner, node)) else {
                    return Ok(());
                };
                if !valid(inner, &owner, true) {
                    self.cancel();
                    return Ok(());
                }
                let initial = super::source_callbacks::event_value(&owner.node)?;
                self.interaction
                    .begin(owner, RangeGesture::Key(key.key()), &initial)?;
                self.pointer_released = false;
                self.pointer_changed = false;
            }
            "keyup" => {
                if current.as_ref() != self.interaction.owner().map(|owner| &owner.node) {
                    return Ok(());
                }
                if let Some(key) = event
                    .dyn_ref::<KeyboardEvent>()
                    .filter(|key| !key.is_composing())
                {
                    self.finish(inner, &RangeGesture::Key(key.key()))?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
fn owner(inner: &Inner, node: &Element) -> Option<Owner> {
    if node.dyn_ref::<HtmlInputElement>()?.type_() != "range" {
        return None;
    }
    let path = inner
        .frame
        .nodes
        .iter()
        .find(|(_, owned)| owned.is_same_node(Some(node.as_ref())))?
        .0
        .clone();
    let callbacks = inner.callbacks.as_ref()?;
    Some(Owner {
        binding: callbacks.event_owner(&DocumentCallbacks::value_commit_key(&path))?,
        path,
        node: node.clone(),
        scope: callbacks.scope().clone(),
        definition: inner.program.definition_revision(),
    })
}
fn valid(inner: &Inner, owner: &Owner, focused: bool) -> bool {
    if inner.closed
        || inner.poisoned
        || !owner.binding.is_active()
        || !inner.shell.contains(Some(owner.node.as_ref()))
        || inner.program.definition_revision() != owner.definition
    {
        return false;
    }
    let Some(callbacks) = inner.callbacks.as_ref() else {
        return false;
    };
    if callbacks.scope() != &owner.scope
        || callbacks
            .event_owner(&DocumentCallbacks::value_commit_key(&owner.path))
            .as_ref()
            != Some(&owner.binding)
    {
        return false;
    }
    if !inner
        .frame
        .nodes
        .get(&owner.path)
        .is_some_and(|node| node.is_same_node(Some(owner.node.as_ref())))
    {
        return false;
    }
    if owner.node.matches(":disabled").unwrap_or(true)
        || !matches!(
            owner
                .node
                .closest("[hidden],[inert],[aria-disabled='true']"),
            Ok(None)
        )
    {
        return false;
    }
    if focused
        && owner
            .node
            .owner_document()
            .and_then(|doc| doc.active_element())
            .as_ref()
            != Some(&owner.node)
    {
        return false;
    }
    true
}
impl WebDocument {
    pub(super) fn install_range_commit_dispatcher(&mut self) -> Result<(), String> {
        for kind in [
            "pointerdown",
            "pointerup",
            "pointercancel",
            "pointerleave",
            "keydown",
            "keyup",
            "focusout",
            "change",
        ] {
            let weak = Rc::downgrade(&self.inner);
            self.listeners.push(Listener::capture(
                self.inner.borrow().shell.clone().unchecked_into(),
                kind,
                move |event| {
                    let Some(owner) = weak.upgrade() else {
                        return;
                    };
                    let mut inner = owner.borrow_mut();
                    let mut state = std::mem::take(&mut inner.range_commit);
                    let result = state.event(&mut inner, &event);
                    inner.range_commit = state;
                    if let Err(error) = result {
                        inner.last_error = Some(error);
                    }
                },
            )?);
        }
        Ok(())
    }
}

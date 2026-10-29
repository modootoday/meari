//! Native node admission around the shared document callback queue.
mod boolean;
mod selection;
pub use meari::application::host_callbacks::PendingCallback;
use meari::{
    application::host_callbacks::{CallbackBindings, DocumentCallbacks},
    domain::{
        host_callbacks::{BoundCallback, CallbackSource},
        plugins::PluginScope,
    },
};
use std::collections::BTreeMap;

pub(super) struct CallbackSession {
    queue: DocumentCallbacks,
    nodes: BTreeMap<String, blitz_dom::NodeId>,
    targets: BTreeMap<String, blitz_dom::NodeId>,
}
impl std::ops::Deref for CallbackSession {
    type Target = DocumentCallbacks;
    fn deref(&self) -> &Self::Target {
        &self.queue
    }
}
impl std::ops::DerefMut for CallbackSession {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.queue
    }
}
impl CallbackSession {
    pub fn new(
        bindings: CallbackBindings,
        expected: &PluginScope,
        handlers: BTreeMap<String, BoundCallback>,
    ) -> Result<Self, String> {
        Ok(Self {
            queue: DocumentCallbacks::new(bindings, expected, handlers)?,
            nodes: BTreeMap::new(),
            targets: BTreeMap::new(),
        })
    }
    pub fn own_nodes(&mut self, document: &InteractiveDocument) -> Result<(), String> {
        let targets = document
            .elements()
            .iter()
            .map(|element| {
                let id = document
                    .renderer
                    .document
                    .get_element_by_id(&element.id)
                    .ok_or("callback action target DOM node missing")?;
                Ok(std::iter::once(&element.path)
                    .chain(&element.aliases)
                    .map(|path| (path.clone(), id))
                    .collect::<Vec<_>>())
            })
            .collect::<Result<Vec<_>, String>>()?
            .into_iter()
            .flatten()
            .collect();
        self.nodes = self
            .queue
            .paths()
            .map(|path| {
                let id = document
                    .path_id(
                        self.queue
                            .node_path(path)
                            .ok_or("callback slot is not registered")?,
                    )
                    .and_then(|id| document.renderer.document.get_element_by_id(id))
                    .ok_or("callback owned DOM node missing")?;
                Ok((path.clone(), id))
            })
            .collect::<Result<_, String>>()?;
        self.targets = targets;
        Ok(())
    }
    pub fn validate_owned_nodes(&self, document: &InteractiveDocument) -> Result<(), String> {
        for (path, original) in &self.nodes {
            let current = self
                .queue
                .node_path(path)
                .and_then(|path| document.path_id(path))
                .and_then(|id| document.renderer.document.get_element_by_id(id));
            if current != Some(*original) {
                return Err("retained callback original node changed".into());
            }
        }
        Ok(())
    }
    pub fn scope(&self) -> &PluginScope {
        self.queue.scope()
    }
    pub fn drain(&mut self) -> Vec<PendingCallback> {
        self.queue.drain()
    }
    pub fn is_text(&self, path: &str) -> bool {
        self.queue.source(path) == Some(CallbackSource::CurrentTargetText)
    }
    pub fn is_action(&self, path: &str) -> bool {
        matches!(
            self.queue.source(path),
            Some(CallbackSource::Action | CallbackSource::CapturedText)
        )
    }
    pub fn is_boolean(&self, path: &str) -> bool {
        self.queue.source(path) == Some(CallbackSource::BooleanValue)
    }
    pub fn is_selection(&self, path: &str) -> bool {
        self.queue.source(path) == Some(CallbackSource::SelectionValue)
    }
}
use super::document::InteractiveDocument;
use crate::domain::widgets::Action;

impl InteractiveDocument {
    pub(super) fn range_commit_owners(
        &self,
    ) -> BTreeMap<String, meari::application::host_callbacks::CallbackEventOwner> {
        self.callbacks
            .as_ref()
            .map(|callbacks| {
                callbacks
                    .paths()
                    .filter_map(|key| {
                        let node = callbacks.node_path(key)?;
                        if key != &DocumentCallbacks::value_commit_key(node) {
                            return None;
                        }
                        Some((node.to_owned(), callbacks.event_owner(key)?))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub(super) fn enqueue_range_commit(&mut self, path: &str, actual: &str) -> Result<(), String> {
        let key = DocumentCallbacks::value_commit_key(path);
        let Some(callbacks) = &self.callbacks else {
            return Ok(());
        };
        if callbacks.source(&key).is_none() {
            return Ok(());
        }
        let id = self
            .path_id(path)
            .and_then(|id| self.renderer.document.get_element_by_id(id))
            .ok_or("range commit owned node missing")?;
        if callbacks.nodes.get(&key) != Some(&id) {
            return Err("range commit original node replaced".into());
        }
        let node = self
            .renderer
            .document
            .get_node(id)
            .ok_or("range commit node missing")?;
        if node.attr(blitz_dom::LocalName::from("type")) != Some("range") {
            return Err("value commit requires original range control".into());
        }
        self.admit_callback_target(path, Some("text"), true)?;
        self.callbacks
            .as_mut()
            .expect("checked callback owner")
            .enqueue_guarded(&key, Some(actual))
    }
    pub fn callback_scope(&self) -> Option<&PluginScope> {
        self.callbacks.as_ref().map(CallbackSession::scope)
    }

    /// No registry borrow occurs here. Dispatch each request after returning to the host.
    pub fn drain_callbacks(&mut self) -> Vec<PendingCallback> {
        let ready = self
            .callbacks
            .as_mut()
            .map(CallbackSession::drain)
            .unwrap_or_default();
        let epoch = self
            .callback_scope()
            .map_or(self.application_generation, |scope| scope.source_generation);
        let definition = self
            .state_program
            .as_ref()
            .map_or(self.application_definition_revision, |program| {
                program.definition_revision()
            });
        let owners = self.range_commit_owners();
        if let Some((path, value)) =
            self.range_inputs
                .pending(&self.renderer.document, epoch, definition, &owners)
        {
            match self.enqueue_range_commit(&path, &value) {
                Ok(()) => {
                    self.range_inputs.acknowledge();
                }
                Err(error) => self.last_error = Some(error),
            }
        }
        ready
    }
    /// A bounded drain may stage the original terminal intent for the next turn.
    pub fn has_pending_callbacks(&self) -> bool {
        self.callbacks
            .as_ref()
            .is_some_and(|callbacks| callbacks.has_pending())
    }

    /// Revoke even already-drained handles. Registry tickets must be cancelled/drained by its owner.
    pub fn close_callbacks(&mut self) {
        self.range_inputs.cancel();
        self.callbacks = None;
    }

    pub(super) fn has_text_callback(&self, path: &str) -> bool {
        self.callbacks
            .as_ref()
            .is_some_and(|callbacks| callbacks.is_text(path))
            || self.module_program.as_ref().is_some_and(|program| {
                program.project().is_ok_and(|projection| {
                    projection
                        .callback_bindings()
                        .get(path)
                        .is_some_and(|handler| {
                            handler.source
                                == meari::domain::host_callbacks::CallbackSource::CurrentTargetText
                        })
                })
            })
    }

    fn admit_callback(&self, path: &str, text: bool) -> Result<(), String> {
        self.admit_callback_owner(path)?;
        self.admit_callback_target(path, Some(if text { "text" } else { "button" }), text)
    }

    fn admit_callback_owner(&self, path: &str) -> Result<(), String> {
        if self.source_input_poisoned {
            return Err("callback document requires remount after failed recovery".into());
        }
        let target = self
            .elements()
            .iter()
            .find(|element| element.owns_path(path))
            .ok_or("callback target is not owned by this document")?;
        let id = self
            .renderer
            .document
            .get_element_by_id(&target.id)
            .ok_or("callback target DOM identity was lost")?;
        if self
            .callbacks
            .as_ref()
            .and_then(|callbacks| callbacks.nodes.get(path))
            != Some(&id)
        {
            return Err("callback owned node was replaced".into());
        }
        Ok(())
    }

    pub(in crate::infrastructure) fn admit_callback_target(
        &self,
        path: &str,
        kind: Option<&str>,
        read_only: bool,
    ) -> Result<(), String> {
        let target = self
            .elements()
            .iter()
            .find(|element| element.owns_path(path))
            .ok_or("callback target is not owned by this document")?;
        let id = self
            .renderer
            .document
            .get_element_by_id(&target.id)
            .ok_or("callback target DOM identity was lost")?;
        let node = self
            .renderer
            .document
            .get_node(id)
            .ok_or("callback target missing")?;
        if node.attr(blitz_dom::LocalName::from("data-bridge-path")) != Some(target.path.as_str()) {
            return Err("callback target path identity differs".into());
        }
        let tag: &str = node
            .element_data()
            .ok_or("callback target is not an element")?
            .name
            .local
            .as_ref();
        if (kind == Some("text") && !["input", "textarea"].contains(&tag))
            || (kind == Some("button") && tag != "button")
        {
            return Err("callback event does not match the native control kind".into());
        }
        for ancestor in self.elements().iter().filter(|element| {
            element.path == path || path.starts_with(&format!("{}/", element.path))
        }) {
            let dom = self
                .renderer
                .document
                .get_element_by_id(&ancestor.id)
                .and_then(|id| self.renderer.document.get_node(id))
                .ok_or("callback ancestor DOM identity was lost")?;
            let attr = |name: &str| dom.attr(blitz_dom::LocalName::from(name));
            if attr("hidden").is_some()
                || attr("inert").is_some()
                || attr("aria-disabled") == Some("true")
                || (ancestor.owns_path(path)
                    && (attr("disabled").is_some() || (read_only && attr("readonly").is_some())))
                || (dom
                    .element_data()
                    .is_some_and(|data| data.name.local == blitz_dom::local_name!("fieldset"))
                    && attr("disabled").is_some())
            {
                return Err("callback target is disabled, readOnly, hidden or inert".into());
            }
            if let Some(styles) = dom.primary_styles()
                && (styles.clone_display().is_none()
                    || styles.get_inherited_box().visibility != style::properties::generated::longhands::visibility::computed_value::T::Visible)
            {
                return Err("callback target is hidden by computed CSS".into());
            }
        }
        Ok(())
    }

    pub fn enqueue_callback_text(&mut self, path: &str, value: &str) -> Result<(), String> {
        if !self.has_text_callback(path) {
            return Err("text callback path is not registered".into());
        }
        self.admit_callback(path, true)?;
        self.callbacks
            .as_mut()
            .ok_or("callback dispatcher missing")?
            .enqueue_guarded(path, Some(value))
    }

    pub(super) fn dispatch_callback(&mut self, action: &Action) -> Result<Option<bool>, String> {
        let path = match action {
            Action::Activate { path } => path,
            Action::Key { path, key, .. } if key == "Enter" || key == " " => path,
            _ => return Ok(None),
        };
        let registered = self
            .callbacks
            .as_ref()
            .is_some_and(|callbacks| callbacks.is_action(path))
            || self.module_program.as_ref().is_some_and(|program| {
                program.project().is_ok_and(|projection| {
                    projection
                        .callback_bindings()
                        .get(path)
                        .is_some_and(|handler| {
                            matches!(
                                handler.source,
                                meari::domain::host_callbacks::CallbackSource::Action
                                    | meari::domain::host_callbacks::CallbackSource::CapturedText
                            )
                        })
                })
            });
        if !registered {
            return Ok(None);
        }
        self.admit_callback(path, false)?;
        self.callbacks
            .as_mut()
            .ok_or("callback dispatcher missing")?
            .enqueue_guarded(path, None)?;
        Ok(Some(true))
    }
}

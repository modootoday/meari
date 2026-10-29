//! Scoped Rust listeners and shared document callback leases; no registry calls in listeners.
use super::{WebDocument, frame::error};
use crate::{
    application::{
        dom,
        host_callbacks::{CallbackBindings, DocumentCallbacks, PendingCallback},
        source_modules::ModuleProgram,
    },
    domain::{Value, host_callbacks::CallbackSource, plugins::PluginScope},
};
use std::collections::BTreeMap;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlInputElement, HtmlTextAreaElement};

fn message(error: crate::application::source_modules::ModuleDiagnostic) -> String {
    format!(
        "{}:{}..{}: {}",
        error.file.as_str(),
        error.diagnostic.span.start,
        error.diagnostic.span.end,
        error.diagnostic.message
    )
}
impl super::Inner {
    pub(super) fn enqueue_callback(
        &mut self,
        path: &str,
        current: &Element,
        text: Option<&str>,
    ) -> Result<(), String> {
        if self.closed || self.poisoned {
            return Err("Web callback document is closed or requires recovery".into());
        }
        self.frame.preflight(&self.frame.plan, &self.shell)?;
        let owned = self
            .frame
            .nodes
            .get(
                self.callbacks
                    .as_ref()
                    .and_then(|callbacks| callbacks.node_path(path))
                    .ok_or("Web callback slot is not registered")?,
            )
            .ok_or("Web callback node missing")?;
        if !owned.is_same_node(Some(current.as_ref())) || !self.shell.contains(Some(owned)) {
            return Err("Web callback current target is not the owned node".into());
        }
        let callbacks = self
            .callbacks
            .as_ref()
            .ok_or("Web callback dispatcher missing")?;
        let source = callbacks
            .source(path)
            .ok_or("Web callback handler missing")?;
        if (matches!(
            source,
            CallbackSource::Action | CallbackSource::CapturedText
        ) && current.local_name() != "button")
            || (source == CallbackSource::CurrentTargetText
                && !["input", "textarea"].contains(&current.local_name().as_str()))
        {
            return Err("Web callback kind differs from its control".into());
        }
        if current.matches(":disabled").map_err(error)?
            || current
                .closest("[hidden],[inert],[aria-disabled='true']")
                .map_err(error)?
                .is_some()
            || (source == CallbackSource::CurrentTargetText && current.has_attribute("readonly"))
        {
            return Err("Web callback target is disabled, readOnly, hidden or inert".into());
        }
        let window = current
            .owner_document()
            .and_then(|document| document.default_view())
            .ok_or("Web callback window missing")?;
        let mut ancestor = Some(current.clone());
        while let Some(element) = ancestor {
            let style = window
                .get_computed_style(&element)
                .map_err(error)?
                .ok_or("Web callback computed style missing")?;
            if style.get_property_value("display").map_err(error)? == "none" {
                return Err("Web callback target is hidden by computed CSS".into());
            }
            if element.is_same_node(Some(current.as_ref()))
                && ["hidden", "collapse"].contains(
                    &style
                        .get_property_value("visibility")
                        .map_err(error)?
                        .as_str(),
                )
            {
                return Err("Web callback target is not visible".into());
            }
            ancestor = element.parent_element();
        }
        self.callbacks
            .as_mut()
            .ok_or("Web callback dispatcher missing")?
            .enqueue_guarded(path, text)
    }
}
pub(super) fn event_value(element: &Element) -> Result<String, String> {
    if let Some(input) = element.dyn_ref::<HtmlInputElement>() {
        if [
            "checkbox", "radio", "file", "button", "submit", "reset", "image", "hidden",
        ]
        .contains(&input.type_().as_str())
        {
            return Err("Web callback requires a text-valued control".into());
        }
        Ok(input.value())
    } else if let Some(input) = element.dyn_ref::<HtmlTextAreaElement>() {
        Ok(input.value())
    } else {
        Err("Web callback input is not a text control".into())
    }
}

impl WebDocument {
    pub fn mount_module_program_with_callbacks(
        container: &Element,
        program: ModuleProgram,
        css: &str,
        identity: (String, u64),
        source_generation: u64,
        export: impl FnOnce(PluginScope) -> Result<CallbackBindings, String>,
    ) -> Result<Self, String> {
        for definition in program.definitions() {
            dom::validate_source_callbacks(definition, true)?;
        }
        let projection = program.project().map_err(message)?;
        if projection.input_paths().next().is_some() {
            return Err("Web state input events require the typed state-input adapter".into());
        }
        let scope = PluginScope {
            instance: projection
                .instances
                .get("root")
                .ok_or("Web module root missing")?
                .instance
                .clone(),
            source_generation,
        };
        if source_generation == 0 {
            return Err("Web callback generation must be positive".into());
        }
        let handlers = projection.callback_bindings();
        let mut document = Self::mount_internal(
            container,
            &projection.component,
            &BTreeMap::new(),
            css,
            identity,
            true,
        )?;
        let callbacks = DocumentCallbacks::new(export(scope.clone())?, &scope, handlers)?;
        {
            let mut inner = document.inner.borrow_mut();
            inner.frame.sync_text_controls(&callbacks.text_paths())?;
            inner.module_program = Some(program);
            inner.callbacks = Some(callbacks);
        }
        document.install_input_dispatcher()?;
        document.install_range_commit_dispatcher()?;
        Ok(document)
    }
    pub fn callback_scope(&self) -> Option<PluginScope> {
        self.inner
            .borrow()
            .callbacks
            .as_ref()
            .map(|callbacks| callbacks.scope().clone())
    }
    pub fn drain_callbacks(&mut self) -> Vec<PendingCallback> {
        let mut inner = self.inner.borrow_mut();
        let ready = inner
            .callbacks
            .as_mut()
            .map(DocumentCallbacks::drain)
            .unwrap_or_default();
        let mut range = std::mem::take(&mut inner.range_commit);
        if let Err(error) = range.flush(&mut inner) {
            inner.last_error = Some(error);
        }
        inner.range_commit = range;
        ready
    }
    /// Hosts must continue bounded drains while true, before yielding their event turn.
    /// This does not run callbacks in listeners or require another physical input.
    pub fn has_pending_callbacks(&self) -> bool {
        self.inner
            .borrow()
            .callbacks
            .as_ref()
            .is_some_and(DocumentCallbacks::has_pending)
    }
    /// Retain the original live export for range value-only controlled feedback.
    pub fn update_module_range_props(
        &mut self,
        props: BTreeMap<String, Value>,
    ) -> Result<(), String> {
        self.update_module_range_props_with_text(props, &Default::default())
    }
    /// Existing display text content can change without replacing gesture authority.
    pub fn update_module_range_props_with_text(
        &mut self,
        props: BTreeMap<String, Value>,
        display_text_paths: &std::collections::BTreeSet<String>,
    ) -> Result<(), String> {
        let mut inner = self.inner.borrow_mut();
        if inner.closed || inner.poisoned {
            return Err("Web callback document is closed or requires recovery".into());
        }
        let mut program = inner
            .module_program
            .as_ref()
            .ok_or("Web module graph missing")?
            .clone();
        let previous = program.project().map_err(message)?.clone();
        let projection = program.update_props(props).map_err(message)?;
        let callbacks = inner
            .callbacks
            .as_ref()
            .ok_or("Web callback dispatcher missing")?;
        callbacks.validate_retained_range_props_with_text(
            &previous.component,
            &projection.component,
            &projection.callback_bindings(),
            display_text_paths,
        )?;
        inner.frame.preflight(&inner.frame.plan, &inner.shell)?;
        let original = callbacks
            .paths()
            .map(|key| {
                let path = callbacks
                    .node_path(key)
                    .ok_or("Web callback slot missing")?;
                Ok((
                    path.to_owned(),
                    inner
                        .frame
                        .nodes
                        .get(path)
                        .ok_or("Web callback original node missing")?
                        .clone(),
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let text_paths = callbacks.text_paths();
        let plan = dom::prepare(&projection.component)?;
        let shell = inner.shell.clone();
        if let Err((error, poisoned)) = inner.frame.patch(plan, &shell) {
            inner.poisoned = poisoned;
            return Err(error);
        }
        let retained = original.iter().all(|(path, node)| {
            inner
                .frame
                .nodes
                .get(path)
                .is_some_and(|current| current.is_same_node(Some(node)))
        });
        if !retained {
            inner.poisoned = true;
            inner.range_commit = Default::default();
            return Err("Web retained callback original node changed".into());
        }
        if let Err(error) = inner.frame.sync_text_controls(&text_paths) {
            inner.poisoned = true;
            inner.range_commit = Default::default();
            return Err(error);
        }
        inner.module_program = Some(program);
        inner.last_error = None;
        Ok(())
    }
    pub fn close_callbacks(&mut self) {
        let mut inner = self.inner.borrow_mut();
        inner.range_commit = Default::default();
        inner.callbacks = None;
    }
    pub fn update_module_props_with_callbacks(
        &mut self,
        props: BTreeMap<String, Value>,
        export: CallbackBindings,
    ) -> Result<(), String> {
        let mut inner = self.inner.borrow_mut();
        if inner.closed || inner.poisoned {
            return Err("Web callback document is closed or requires recovery".into());
        }
        let scope = inner
            .callbacks
            .as_ref()
            .ok_or("Web callback dispatcher missing")?
            .scope()
            .clone();
        let mut program = inner
            .module_program
            .as_ref()
            .ok_or("Web module graph missing")?
            .clone();
        let projection = program.update_props(props).map_err(message)?;
        let callbacks = DocumentCallbacks::new(export, &scope, projection.callback_bindings())?;
        let plan = dom::prepare(&projection.component)?;
        let shell = inner.shell.clone();
        if let Err((error, poisoned)) = inner.frame.patch(plan, &shell) {
            inner.poisoned = poisoned;
            return Err(error);
        }
        inner.frame.sync_text_controls(&callbacks.text_paths())?;
        inner.module_program = Some(program);
        inner.callbacks = Some(callbacks);
        inner.last_error = None;
        Ok(())
    }
}

//! WASM adapter for persistent browser DOM and shared source-state programs.
mod frame;
mod listener;
use self::controls::radio_group;
use self::controls::range_parts;
use self::sources::source_reload;
pub use source_reload::PreparedSourceReplacement;
mod application_reload;
mod hooks;
mod host_observation;
use self::controls::range_commit;
use self::sources::source_callbacks;
use self::sources::source_events;
use self::sources::source_modules;
mod controls;
mod sources;
pub use application_reload::{CommittedApplicationReplacement, PreparedApplicationReplacement};
pub use hooks::WebHookFailure;

use crate::{
    application::{binding, component_state::StateCommit, dom, state_program::StateProgram},
    domain::{Component, Value, state_program::StateEvent},
};
use frame::{Frame, error};
use listener::Listener;
use std::fmt::Write;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
use wasm_bindgen::JsCast;
use web_sys::Element;

struct Inner {
    range_commit: range_commit::State,
    hook_document: Option<crate::application::hook_document::HookDocument>,
    hook_failure: Option<WebHookFailure>,
    program: StateProgram,
    module_program: Option<crate::application::source_modules::ModuleProgram>,
    callbacks: Option<crate::application::host_callbacks::DocumentCallbacks>,
    frame: Frame,
    events: BTreeMap<String, StateEvent>,
    shell: Element,
    closed: bool,
    poisoned: bool,
    last_error: Option<String>,
    scope: String,
    source_schema: crate::domain::source_schema::SourceSchema,
    application: Option<crate::application::app_bundle::ApplicationDefinition>,
    application_generation: u64,
}

impl Inner {
    fn dispatch(&mut self, path: &str) -> Result<bool, String> {
        if self.closed || self.poisoned {
            return Err("Web document is closed or requires recovery".into());
        }
        if let Some(changed) = self.dispatch_radio(path, None)? {
            return Ok(changed);
        }
        let node = self
            .frame
            .nodes
            .get(path)
            .ok_or("Web event path is missing")?;
        let element = node
            .dyn_ref::<Element>()
            .ok_or("Web event target is not an element")?;
        if self.callbacks.as_ref().is_some_and(|callbacks| {
            matches!(
                callbacks.source(path),
                Some(
                    crate::domain::host_callbacks::CallbackSource::Action
                        | crate::domain::host_callbacks::CallbackSource::CapturedText
                )
            )
        }) {
            let element = element.clone();
            self.enqueue_callback(path, &element, None)?;
            return Ok(true);
        }
        if element.matches(":disabled").map_err(error)?
            || element
                .closest("[hidden],[inert],[aria-disabled='true']")
                .map_err(error)?
                .is_some()
        {
            return Ok(false);
        }
        if self.module_program.is_some() {
            return self.dispatch_module_state(path);
        }
        let event = self
            .events
            .get(path)
            .ok_or("Web event handler is missing")?;
        let mut staged = self.program.clone();
        let projection = staged.dispatch(event)?;
        let plan = dom::prepare(&projection.component)?;
        if let Err((error, poisoned)) = self.frame.patch(plan, &self.shell) {
            self.poisoned = poisoned;
            return Err(error);
        }
        self.program = staged;
        self.events = projection.events;
        self.last_error = None;
        Ok(true)
    }
}

/// One owned DOM mount. Dropping it detaches callbacks and removes only its wrapper.
pub struct WebDocument {
    inner: Rc<RefCell<Inner>>,
    listeners: Vec<Listener>,
    host_observer: Option<crate::infrastructure::web_host::observer::BrowserHostObserver>,
    host_responsive: Option<crate::application::responsive::ResponsiveSession>,
    pending_environment: Option<crate::domain::host::HostEnvironment>,
    media_bindings: Option<crate::application::component_state::media::MediaStateBindings>,
}

impl WebDocument {
    /// Issue a same-parent movable slot under the current document lease.
    pub fn movable_node_slot(
        &self,
        key: &str,
        reference: crate::domain::source_modules::ComponentReference,
        props: BTreeMap<String, Value>,
    ) -> Result<Value, String> {
        let inner = self
            .inner
            .try_borrow()
            .map_err(|_| "web document is borrowed")?;
        if inner.closed || inner.poisoned {
            return Err("web document is closed or poisoned".into());
        }
        inner
            .module_program
            .as_ref()
            .ok_or("Web module program missing")?
            .node_slots()
            .movable_component(key, reference, props)
    }

    pub fn node_slot(
        &self,
        key: &str,
        reference: crate::domain::source_modules::ComponentReference,
        props: BTreeMap<String, Value>,
    ) -> Result<Value, String> {
        let inner = self
            .inner
            .try_borrow()
            .map_err(|_| "web document is borrowed")?;
        if inner.closed || inner.poisoned {
            return Err("web document is closed or poisoned".into());
        }
        inner
            .module_program
            .as_ref()
            .ok_or("document has no module graph")?
            .node_slots()
            .component(key, reference, props)
    }
    pub fn mount_application(
        definition: &crate::application::app_bundle::ApplicationDefinition,
        css: &str,
    ) -> Result<Self, String> {
        definition.validate_target(crate::domain::build::TargetKind::Web)?;
        let document = web_sys::window()
            .ok_or("Window unavailable")?
            .document()
            .ok_or("Document unavailable")?;
        let body = document.body().ok_or("Body unavailable")?;
        let mut mounted = Self::mount(
            body.as_ref(),
            &definition.component,
            &definition.props,
            css,
            format!("meari-{}", definition.stamp.graph_hash.as_str()),
            definition.stamp.generation,
        )?;
        mounted.observe_host()?;
        if crate::infrastructure::web_host::snapshot()?.layout.dark {
            let html = document.document_element().ok_or("HTML root unavailable")?;
            let mut classes = html.get_attribute("class").unwrap_or_default();
            if !classes.split_whitespace().any(|class| class == "dark") {
                classes.push_str(" dark");
                html.set_attribute("class", classes.trim()).map_err(error)?;
            }
        }
        document.set_title(&definition.title);
        mounted
            .inner
            .borrow()
            .shell
            .set_attribute("data-meari-graph", definition.stamp.graph_hash.as_str())
            .map_err(error)?;
        mounted
            .inner
            .borrow()
            .shell
            .set_attribute(
                "data-meari-generation",
                &definition.stamp.generation.to_string(),
            )
            .map_err(error)?;
        mounted.inner.borrow_mut().application = Some(definition.clone());
        mounted.inner.borrow_mut().application_generation = definition.stamp.generation;
        Ok(mounted)
    }

    pub fn mount(
        container: &Element,
        source: &Component,
        props: &BTreeMap<String, Value>,
        css: &str,
        document_id: String,
        epoch: u64,
    ) -> Result<Self, String> {
        Self::mount_internal(container, source, props, css, (document_id, epoch), false)
    }

    fn mount_internal(
        container: &Element,
        source: &Component,
        props: &BTreeMap<String, Value>,
        css: &str,
        identity: (String, u64),
        callbacks: bool,
    ) -> Result<Self, String> {
        let (document_id, epoch) = identity;
        dom::validate_source_callbacks(source, callbacks)?;
        let source_schema = crate::domain::source_schema::SourceSchema::from_component(source)?;
        let mut scope = String::new();
        for byte in format!("{}:{document_id}:{epoch}", document_id.len()).bytes() {
            write!(scope, "{byte:02x}").expect("writing to String");
        }
        let source = binding::bind_with_scope(source, props, Some(&scope))?;
        let program = StateProgram::mount(source, document_id, epoch)?;
        let projection = program.project()?;
        let document = container
            .owner_document()
            .ok_or("Web mount container has no document")?;
        if document
            .query_selector(&format!("[data-meari-scope='{scope}']"))
            .map_err(error)?
            .is_some()
        {
            return Err("Web document scope/epoch is already mounted".into());
        }
        let frame = Frame::build(&document, dom::prepare(&projection.component)?)?;
        let shell = document.create_element("div").map_err(error)?;
        shell
            .set_attribute("data-meari-document", "")
            .map_err(error)?;
        shell
            .set_attribute("data-meari-scope", &scope)
            .map_err(error)?;
        let style = document.create_element("style").map_err(error)?;
        style.set_text_content(Some(css));
        shell.append_child(&style).map_err(error)?;
        shell.append_child(&frame.root).map_err(error)?;
        range_parts::sync_frame(&frame, &shell)?;
        let inner = Rc::new(RefCell::new(Inner {
            range_commit: Default::default(),
            program,
            module_program: None,
            hook_document: None,
            hook_failure: None,
            callbacks: None,
            frame,
            events: projection.events,
            shell: shell.clone(),
            closed: false,
            poisoned: false,
            last_error: None,
            scope,
            source_schema,
            application: None,
            application_generation: 0,
        }));
        container.append_child(&shell).map_err(error)?;
        let mut result = Self {
            inner,
            listeners: Vec::new(),
            host_observer: None,
            host_responsive: None,
            pending_environment: None,
            media_bindings: None,
        };
        result.install_click_dispatcher()?;
        result.install_range_parts_dispatcher()?;
        result.install_radio_keyboard()?;
        result.install_radio_focus()?;
        Ok(result)
    }

    pub fn snapshot(&self) -> Result<StateCommit, String> {
        let inner = self.inner.borrow();
        if inner.hook_document.is_some() {
            return Err("hook roots require the borrowed hook-state reader".into());
        }
        if inner.module_program.is_some() {
            return Err("module graph requires module_snapshot".into());
        }
        inner.program.snapshot()
    }
    pub fn last_error(&self) -> Option<String> {
        self.inner.borrow().last_error.clone()
    }
    pub fn element(&self, path: &str) -> Result<Element, String> {
        self.inner
            .borrow()
            .frame
            .nodes
            .get(path)
            .ok_or("Web element path is missing")?
            .clone()
            .dyn_into()
            .map_err(|_| "Web path is not an element".to_owned())
    }
    pub fn dispatch(&mut self, path: &str) -> Result<bool, String> {
        self.inner.borrow_mut().dispatch(path)
    }
    pub fn close(&mut self) {
        self.stop_observing_host();
        self.host_responsive = None;
        self.media_bindings = None;
        self.listeners.clear();
        let mut inner = self.inner.borrow_mut();
        if !inner.closed {
            inner.closed = true;
            inner.range_commit = Default::default();
            inner.callbacks = None;
            inner.hook_failure = None;
            if let Some(hooks) = inner.hook_document.as_mut() {
                hooks.close();
            }
            inner.shell.remove();
        }
    }
}

impl Drop for WebDocument {
    fn drop(&mut self) {
        self.close();
    }
}

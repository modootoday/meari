use crate::{
    application::{
        Renderer,
        markup::{self, Element},
    },
    domain::{
        Viewport,
        widgets::{Action, Family, Session},
    },
    infrastructure::{engine::NativeRenderer, navigation::PublishedNavigation},
};
use blitz_dom::{DocGuard, DocGuardMut, Document, EventDriver, EventHandler, NodeId};
use blitz_traits::events::{DomEvent, DomEventData, EventState, UiEvent};
use meari::domain::Component;
use std::collections::BTreeMap;

/// One persistent native DOM. Widget updates patch attributes/text, preserving input/scroll nodes.
pub struct InteractiveDocument {
    pub renderer: NativeRenderer,
    pub session: Session,
    pub source: Component,
    pub viewport: Viewport,
    pub last_error: Option<String>,
    pub input_values: BTreeMap<String, String>,
    pub(super) range_inputs: super::range_input::RangeInputs,
    pub activation_requests: Vec<String>,
    pub file_values: BTreeMap<String, Vec<crate::application::file_picker::SelectedFile>>,
    pub file_requests: Vec<String>,
    pub checked_values: BTreeMap<String, bool>,
    pub(super) disclosure_values: BTreeMap<String, bool>,
    pub(super) elements: Vec<Element>,
    pub(super) responsive: Option<meari::application::responsive::ResponsiveSession>,
    pub(super) media_bindings:
        Option<meari::application::component_state::media::MediaStateBindings>,
    pub media_changes: Vec<meari::application::media::MediaChange>,
    pub(super) state_program: Option<meari::application::state_program::StateProgram>,
    pub(super) module_program: Option<meari::application::source_modules::ModuleProgram>,
    pub(super) hook_actions: Vec<meari::application::hook_document::BoundAction>,
    pub(super) state_events: BTreeMap<String, meari::domain::state_program::StateEvent>,
    pub(super) state_inputs: BTreeMap<String, meari::domain::input_event::InputStateBinding>,
    pub(super) source_input_poisoned: bool,
    pub(super) callbacks: Option<super::source_callbacks::CallbackSession>,
    pub(super) source_schema: meari::domain::source_schema::SourceSchema,
    pub(super) application: Option<meari::application::app_bundle::ApplicationDefinition>,
    pub(super) application_generation: u64,
    pub(super) application_definition_revision: u64,
}

impl InteractiveDocument {
    /// Read only: the original currently owned/focused range gesture is active.
    /// This is not an OS pressed-button observation, callback permission or SDK state.
    pub fn has_active_range_gesture(&self) -> bool {
        if self.source_input_poisoned {
            return false;
        }
        let epoch = self
            .callback_scope()
            .map_or(self.application_generation, |scope| scope.source_generation);
        let definition = self
            .state_program
            .as_ref()
            .map_or(self.application_definition_revision, |program| {
                program.definition_revision()
            });
        self.range_inputs.is_active(
            &self.renderer.document,
            epoch,
            definition,
            &self.range_commit_owners(),
        )
    }
    pub(super) fn elements_mut_for_disclosure(&mut self, id: &str) -> Option<&mut Element> {
        self.elements.iter_mut().find(|element| element.id == id)
    }
    pub fn subscribe_media(
        &mut self,
        query: &str,
    ) -> Result<meari::application::media::MediaChange, String> {
        self.responsive
            .as_mut()
            .ok_or("responsive host is not attached")?
            .subscribe(&mut self.renderer, query)
    }

    pub fn unsubscribe_media(&mut self, subscription: u64) -> bool {
        if let Some(bindings) = &mut self.media_bindings {
            bindings.unbind(subscription);
        }
        self.media_changes
            .retain(|change| change.subscription != subscription);
        self.responsive
            .as_mut()
            .is_some_and(|session| session.unsubscribe(subscription))
    }

    pub fn recover_host_environment(&mut self) -> Result<(), String> {
        self.responsive
            .as_mut()
            .ok_or("responsive host is not attached")?
            .recover(&mut self.renderer)
    }
    /// Re-layout the persistent DOM without replacing input, focus or widget state.
    pub fn resize_screen(&mut self, host: &Viewport, sizing_element: &str) -> Result<(), String> {
        if self.responsive.is_some() {
            return Err(
                "attached host sizing requires a ScreenRequest and host acknowledgement".into(),
            );
        }
        let previous = self.viewport.clone();
        match crate::application::screen::layout_screen(
            &mut self.renderer,
            host,
            sizing_element,
            0.0,
        ) {
            Ok(viewport) => {
                self.viewport = viewport;
                Ok(())
            }
            Err(error) => {
                self.renderer.layout(&previous, 0.0)?;
                Err(error)
            }
        }
    }

    pub fn new(source: Component, css: &str, viewport: Viewport) -> Result<Self, String> {
        Self::new_with_optional_assets(source, css, viewport, None)
    }
    pub fn new_with_assets(
        source: Component,
        css: &str,
        viewport: Viewport,
        assets: meari::application::assets::VerifiedAssets,
    ) -> Result<Self, String> {
        Self::new_with_optional_assets(source, css, viewport, Some(assets))
    }
    pub(super) fn new_with_optional_assets(
        source: Component,
        css: &str,
        viewport: Viewport,
        assets: Option<meari::application::assets::VerifiedAssets>,
    ) -> Result<Self, String> {
        viewport.validate()?;
        let source_schema = meari::domain::source_schema::SourceSchema::from_component(&source)?;
        let source = crate::application::bind(&source, &BTreeMap::new())?;
        crate::application::state_program::validate(&source)?;
        let state_program = if source.states.is_empty() {
            None
        } else {
            Some(super::state_program::mount(source.clone())?)
        };
        let (source, state_events, state_inputs) = if let Some(program) = &state_program {
            let projection = program.project()?;
            (projection.component, projection.events, projection.inputs)
        } else {
            (source, BTreeMap::new(), BTreeMap::new())
        };
        let session = Session::from_tree(&source.root)?;
        let markup = markup::build(&source, &session)?;
        if markup
            .elements
            .iter()
            .any(|e| e.id == "__bridge_stylesheet")
        {
            return Err("reserved renderer stylesheet id".into());
        }
        let html = format!(
            "<!doctype html><html class=\"{}\"><head><style id=\"__bridge_stylesheet\"></style></head><body>{}</body></html>",
            if viewport.dark { "dark" } else { "" },
            markup.html
        );
        if assets.is_none()
            && markup.elements.iter().any(|element| {
                element.attributes.contains_key("src") || element.attributes.contains_key("poster")
            })
        {
            return Err(
                "native resource elements require an explicit declared asset provider".into(),
            );
        }
        let mut renderer = match assets {
            Some(assets) => NativeRenderer::from_html_with_assets(&html, assets)?,
            None => NativeRenderer::from_html(&html),
        };
        renderer.validate_asset_css(css)?;
        let style_id = renderer
            .document
            .get_element_by_id("__bridge_stylesheet")
            .ok_or("missing stylesheet node")?;
        {
            let mut mutation = renderer.document.mutate();
            let text = mutation.create_text_node(css);
            mutation.append_children(style_id, &[text]);
        }
        renderer.document.upsert_stylesheet_for_node(style_id);
        renderer.layout(&viewport, 0.0)?;
        let mut document = Self {
            renderer,
            session,
            source,
            viewport,
            last_error: None,
            input_values: BTreeMap::new(),
            range_inputs: Default::default(),
            disclosure_values: BTreeMap::new(),
            activation_requests: Vec::new(),
            file_values: BTreeMap::new(),
            file_requests: Vec::new(),
            checked_values: BTreeMap::new(),
            elements: markup.elements,
            responsive: None,
            media_bindings: None,
            media_changes: Vec::new(),
            state_program,
            module_program: None,
            hook_actions: Vec::new(),
            state_events,
            state_inputs,
            source_input_poisoned: false,
            callbacks: None,
            source_schema,
            application: None,
            application_generation: 0,
            application_definition_revision: 0,
        };
        document.focus_initial_controlled_overlays()?;
        Ok(document)
    }

    pub fn from_application(
        definition: &meari::application::app_bundle::ApplicationDefinition,
        css: &str,
    ) -> Result<Self, String> {
        Self::from_application_and_assets(definition, css, None)
    }
    pub fn from_application_with_assets(
        definition: &meari::application::app_bundle::ApplicationDefinition,
        css: &str,
        assets: meari::application::assets::VerifiedAssets,
    ) -> Result<Self, String> {
        Self::from_application_and_assets(definition, css, Some(assets))
    }
    fn from_application_and_assets(
        definition: &meari::application::app_bundle::ApplicationDefinition,
        css: &str,
        assets: Option<meari::application::assets::VerifiedAssets>,
    ) -> Result<Self, String> {
        definition.validate_target(meari::domain::build::TargetKind::Native)?;
        let source_schema = definition.source_schema()?;
        let mut document = Self::new_with_optional_assets(
            definition.bound_component()?,
            css,
            definition.native_viewport.clone(),
            assets,
        )?;
        document.source_schema = source_schema;
        if document.state_program.is_none() {
            document.state_program = Some(super::state_program::mount(document.source.clone())?);
        }
        document.application = Some(definition.clone());
        document.application_generation = definition.stamp.generation;
        document.application_definition_revision = 1;
        Ok(document)
    }

    pub fn path_id(&self, path: &str) -> Option<&str> {
        self.elements
            .iter()
            .find(|e| e.owns_path(path))
            .map(|e| e.id.as_str())
    }
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }
    /// Replace compiled CSS without recreating nodes, widget state, input or focus.
    pub fn replace_stylesheet(&mut self, css: &str) -> Result<(), String> {
        self.renderer.validate_asset_css(css)?;
        self.renderer.snapshot_stylesheet_change();
        let style = self
            .renderer
            .document
            .get_element_by_id("__bridge_stylesheet")
            .ok_or("missing stylesheet node")?;
        {
            let mut mutation = self.renderer.document.mutate();
            let text = mutation.create_text_node(css);
            mutation.replace_children(style, &[text]);
        }
        self.renderer.document.upsert_stylesheet_for_node(style);
        self.renderer.refresh_styles();
        Ok(())
    }
    pub fn focus_path(&mut self, path: &str) -> Result<(), String> {
        let canonical = self
            .elements
            .iter()
            .find(|element| element.owns_path(path))
            .map(|element| element.path.clone());
        let path = canonical.as_deref().unwrap_or(path);
        if self.session.focus_item(path)? {
            self.patch()?;
        }
        let id = self
            .path_id(path)
            .and_then(|id| self.renderer.document.get_element_by_id(id))
            .ok_or("missing focus target")?;
        self.renderer.document.set_focus_to(id);
        if self.renderer.document.get_focussed_node_id() != Some(id) {
            return Err("native focus request failed".into());
        }
        self.session.focus = Some(path.into());
        self.sync_radio_focus()?;
        Ok(())
    }

    pub fn set_input_value(&mut self, path: &str, value: &str) -> Result<(), String> {
        if self.has_text_callback(path) {
            return self.enqueue_callback_text(path, value);
        }
        if self.state_inputs.contains_key(path) {
            return self.dispatch_source_input(
                path,
                &meari::domain::input_event::InputPayload::Text(value.into()),
            );
        }
        let element = self
            .elements
            .iter()
            .find(|e| e.path == path)
            .ok_or("missing input")?;
        if element.attributes.contains_key("disabled")
            || element.attributes.contains_key("readonly")
        {
            return Err("input is disabled or readOnly".into());
        }
        let id = self
            .renderer
            .document
            .get_element_by_id(&element.id)
            .ok_or("missing input DOM")?;
        let input = self
            .renderer
            .document
            .get_node(id)
            .and_then(|n| n.element_data())
            .is_some_and(|e| {
                e.text_input_data().is_some()
                    || e.attr(blitz_dom::LocalName::from("type")) == Some("range")
            });
        if !input {
            return Err("target is not an editable text input".into());
        }
        self.renderer
            .document
            .mutate()
            .set_attribute(id, name("value"), value);
        self.input_values.insert(path.into(), value.into());
        self.renderer.document.resolve(0.0);
        Ok(())
    }

    pub fn dispatch(&mut self, mut action: Action) -> Result<bool, String> {
        let path = match &mut action {
            Action::Activate { path }
            | Action::Key { path, .. }
            | Action::DismissOutside { path } => path,
        };
        if let Some(element) = self.elements.iter().find(|element| element.owns_path(path)) {
            *path = element.path.clone();
        }
        if let Some(changed) = self.dispatch_disclosure(&action)? {
            return Ok(changed);
        }
        if let Some(changed) = self.dispatch_boolean_callback(&action)? {
            return Ok(changed);
        }
        if let Some(changed) = self.dispatch_selection(&action)? {
            return Ok(changed);
        }
        if let Some(changed) = self.dispatch_callback(&action)? {
            return Ok(changed);
        }
        if let Some(changed) = self.dispatch_module_event(&action)? {
            return Ok(changed);
        }
        if let Some(changed) = self.dispatch_state(&action)? {
            return Ok(changed);
        }
        let input_activation = match &action {
            Action::Activate { path } => Some(path),
            Action::Key { path, key, .. } if key == " " => Some(path),
            _ => None,
        };
        if let Some(path) = input_activation
            && self.state_inputs.get(path).is_some_and(|binding| {
                binding.kind == meari::domain::input_event::InputKind::Checked
            })
        {
            let checked = self
                .path_id(path)
                .and_then(|id| self.renderer.document.get_element_by_id(id))
                .and_then(|id| self.renderer.document.get_node(id))
                .and_then(|node| node.element_data())
                .and_then(|data| data.checkbox_input_checked())
                .ok_or("source checkbox DOM missing")?;
            self.dispatch_source_input(
                path,
                &meari::domain::input_event::InputPayload::Checked(!checked),
            )?;
            return Ok(true);
        }
        let toggle_path = match &action {
            Action::Activate { path } => Some(path),
            Action::Key { path, key, .. } if key == " " || key == "Enter" => Some(path),
            _ => None,
        };
        if let Some(path) = toggle_path
            && let Some(element) = self
                .elements
                .iter()
                .find(|e| e.path == *path && e.attributes.contains_key("data-toggle"))
        {
            if element
                .attributes
                .contains_key("data-meari-controlled-toggle")
            {
                if matches!(&action, Action::Key { key, .. } if key == "Enter") {
                    return Ok(false);
                }
                return Err("controlled toggle requires a scoped Boolean intent dispatcher".into());
            }
            if element.attributes.contains_key("disabled") {
                return Ok(false);
            }
            let checked = element
                .attributes
                .get("aria-checked")
                .is_some_and(|v| v == "true");
            self.checked_values.insert(path.clone(), !checked);
            self.patch()?;
            return Ok(true);
        }
        if let Action::Activate { path } = &action
            && !self.session.is_control(path)
        {
            let element = self
                .elements
                .iter()
                .find(|element| element.path == *path)
                .ok_or("missing activation target")?;
            let button = self
                .renderer
                .document
                .get_element_by_id(&element.id)
                .and_then(|id| self.renderer.document.get_node(id))
                .and_then(|node| node.element_data())
                .is_some_and(|element| element.name.local == blitz_dom::local_name!("button"));
            if button && !element.attributes.contains_key("disabled") {
                if element.attributes.contains_key("data-file-input") {
                    self.file_requests.push(path.clone());
                    return Ok(true);
                }
                self.activation_requests.push(path.clone());
                return Ok(true);
            }
        }
        let path = match &action {
            Action::Activate { path }
            | Action::Key { path, .. }
            | Action::DismissOutside { path } => path,
        };
        let scope = self.session.owner(path).and_then(|w| w.content.clone());
        let focusable = self.focusable(scope.as_deref());
        if !self
            .session
            .dispatch(action, &PublishedNavigation, &focusable)?
        {
            return Ok(false);
        }
        self.patch()?;
        Ok(true)
    }

    pub fn choose_files(
        &mut self,
        path: &str,
        picker: &mut dyn crate::application::file_picker::FilePicker,
    ) -> Result<(), String> {
        use crate::application::file_picker::FileRequest;
        let element = self
            .elements
            .iter()
            .find(|e| e.path == path)
            .ok_or("missing file input")?;
        if !element.attributes.contains_key("data-file-input")
            || element.attributes.contains_key("disabled")
        {
            return Err("target is not an enabled file input".into());
        }
        let request = FileRequest {
            multiple: element.attributes.contains_key("multiple"),
            accept: element
                .attributes
                .get("accept")
                .cloned()
                .unwrap_or_default(),
        };
        if let Some(files) = picker.select(&request)? {
            if !request.multiple && files.len() > 1 {
                return Err("single file input received multiple files".into());
            }
            if files
                .iter()
                .any(|f| f.name.is_empty() || f.path.file_name().is_none())
            {
                return Err("invalid selected file metadata".into());
            }
            let label = if files.is_empty() {
                "No file chosen".into()
            } else {
                files
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let id = self
                .renderer
                .document
                .get_element_by_id(&element.id)
                .ok_or("missing file input DOM")?;
            let mut mutation = self.renderer.document.mutate();
            mutation.remove_and_drop_all_children(id);
            let text = mutation.create_text_node(&format!("Choose files — {label}"));
            mutation.append_children(id, &[text]);
            self.file_values.insert(path.into(), files);
        }
        self.renderer.document.resolve(0.0);
        self.focus_path(path)?;
        Ok(())
    }

    pub fn process_file_requests(
        &mut self,
        picker: &mut dyn crate::application::file_picker::FilePicker,
    ) -> Result<(), String> {
        for path in std::mem::take(&mut self.file_requests) {
            self.choose_files(&path, picker)?;
        }
        Ok(())
    }

    pub(super) fn focusable(&self, scope: Option<&str>) -> Vec<String> {
        self.elements
            .iter()
            .filter(|element| {
                let in_scope =
                    scope.is_none_or(|scope| element.path.starts_with(&format!("{scope}/")));
                let disabled = element.attributes.contains_key("disabled")
                    || element
                        .attributes
                        .get("aria-disabled")
                        .is_some_and(|s| s == "true");
                let negative = element
                    .attributes
                    .get("tabindex")
                    .is_some_and(|s| s == "-1");
                let hidden = self.elements.iter().any(|ancestor| {
                    (element.path == ancestor.path
                        || element.path.starts_with(&format!("{}/", ancestor.path)))
                        && ancestor.attributes.contains_key("hidden")
                });
                in_scope
                    && !disabled
                    && !negative
                    && !hidden
                    && self
                        .admit_callback_target(&element.path, None, false)
                        .is_ok()
                    && self
                        .renderer
                        .document
                        .get_element_by_id(&element.id)
                        .and_then(|id| self.renderer.document.get_node(id))
                        .is_some_and(|node| node.is_focussable())
            })
            .map(|e| e.path.clone())
            .collect()
    }

    pub(super) fn patch(&mut self) -> Result<(), String> {
        let mut next = markup::build(&self.source, &self.session)?;
        for element in &mut next.elements {
            if let Some(open) = self.disclosure_values.get(&element.id) {
                if *open {
                    element.attributes.insert("open".into(), String::new());
                } else {
                    element.attributes.remove("open");
                }
            }
            if !element
                .attributes
                .contains_key("data-meari-controlled-toggle")
                && let Some(checked) = self.checked_values.get(&element.path)
            {
                element
                    .attributes
                    .insert("aria-checked".into(), checked.to_string());
                element.attributes.insert(
                    "data-state".into(),
                    if *checked { "checked" } else { "unchecked" }.into(),
                );
            }
        }
        if self.elements.iter().chain(&next.elements).any(|element| {
            element
                .attributes
                .contains_key(meari::application::repeat::IDENTITY_ATTRIBUTE)
        }) {
            return self.patch_keyed(next);
        }
        if next.elements.len() != self.elements.len() {
            return Err("source element shape changed".into());
        }
        let nodes: Vec<_> = next
            .elements
            .iter()
            .map(|element| {
                let old = self
                    .elements
                    .iter()
                    .find(|old| old.path == element.path)
                    .ok_or("source shape changed during state update")?;
                if !old.same_composition(element) {
                    return Err("native child composition changed; remount is required");
                }
                self.renderer
                    .document
                    .get_element_by_id(&old.id)
                    .ok_or("native DOM identity lost")
            })
            .collect::<Result<_, &str>>()?;
        let text_updates =
            super::text_patch::prepare(&self.renderer.document, &self.elements, &next.elements)?;
        self.renderer
            .validate_asset_references(next.elements.iter().flat_map(|element| {
                ["src", "poster"]
                    .into_iter()
                    .filter_map(|name| element.attributes.get(name).map(String::as_str))
            }))?;
        // Validate live control kinds before making any DOM changes.
        for (element, node) in next.elements.iter().zip(&nodes) {
            if self.state_inputs.contains_key(&element.path)
                || self.has_text_callback(&element.path)
            {
                let data = self
                    .renderer
                    .document
                    .get_node(*node)
                    .and_then(|node| node.element_data())
                    .ok_or("source input DOM missing")?;
                if element.controlled_checked.is_some() {
                    if data.checkbox_input_checked().is_none() {
                        return Err("source checkbox DOM kind changed".into());
                    }
                } else if element.dom_tag == "input"
                    && element.attributes.get("type").map(String::as_str) == Some("range")
                {
                    if !super::range_input::is_range(&self.renderer.document, *node) {
                        return Err("source range input DOM kind changed".into());
                    }
                } else if data.text_input_data().is_none() {
                    return Err("source text input DOM kind changed".into());
                }
            }
        }
        for (element, node) in next.elements.iter().zip(nodes) {
            let controlled_input = self.state_inputs.contains_key(&element.path)
                || self.has_text_callback(&element.path);
            let previous = self
                .elements
                .iter()
                .find(|e| e.path == element.path)
                .ok_or("source shape changed during widget state update")?;
            let mut mutation = self.renderer.document.mutate();
            for key in previous
                .attributes
                .keys()
                .filter(|key| !element.attributes.contains_key(*key))
            {
                mutation.clear_attribute(node, name(key));
            }
            for (key, value) in &element.attributes {
                if previous.attributes.get(key) != Some(value) {
                    if key == "value"
                        && element.preserve_dirty_value
                        && self.input_values.contains_key(&element.path)
                    {
                        continue;
                    }
                    mutation.set_attribute(node, name(key), value);
                }
            }
            // A dirty editor can differ even when the markup attribute is unchanged.
            if controlled_input {
                if let Some(checked) = element.controlled_checked {
                    mutation.set_attribute(
                        node,
                        name("checked"),
                        if checked { "true" } else { "false" },
                    );
                    if !checked {
                        mutation.clear_attribute(node, name("checked"));
                    }
                } else if let Some(value) = element.attributes.get("value") {
                    mutation.set_attribute(node, name("value"), value);
                }
            }
            if previous.text_override != element.text_override
                && let Some(text) = &element.text_override
            {
                mutation.remove_and_drop_all_children(node);
                let text = mutation.create_text_node(text);
                mutation.append_children(node, &[text]);
            }
        }
        super::text_patch::apply(&mut self.renderer.document, text_updates);
        self.elements = next.elements;
        for element in &self.elements {
            if self.state_inputs.contains_key(&element.path)
                && let Some(value) = element.attributes.get("value")
            {
                self.input_values
                    .insert(element.path.clone(), value.clone());
            }
        }
        let viewport = self.renderer.document.get_viewport();
        let (width, height) = viewport.logical_size();
        self.viewport.width = width.round().max(1.0) as u32;
        self.viewport.height = height.round().max(1.0) as u32;
        // Preserve the shell's DPI/zoom rather than replacing it with a headless scale.
        self.renderer.document.resolve(0.0);
        if let Some(path) = self.session.focus.as_deref()
            && let Some(id) = self
                .path_id(path)
                .and_then(|id| self.renderer.document.get_element_by_id(id))
        {
            self.renderer.document.set_focus_to(id);
        }
        Ok(())
    }
}

pub(super) fn name(value: &str) -> blitz_dom::QualName {
    blitz_dom::QualName::new(
        None,
        blitz_dom::Namespace::from(""),
        blitz_dom::LocalName::from(value),
    )
}

struct QueuedHandler<'a> {
    session: &'a Session,
    actions: &'a mut Vec<Action>,
    inputs: &'a mut BTreeMap<String, String>,
    source_inputs: &'a BTreeMap<String, meari::domain::input_event::InputStateBinding>,
    input_events: &'a mut Vec<(String, meari::domain::input_event::InputPayload)>,
    callback_inputs: &'a std::collections::BTreeSet<String>,
}
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

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
    pub activation_requests: Vec<String>,
    pub file_values: BTreeMap<String, Vec<crate::application::file_picker::SelectedFile>>,
    pub file_requests: Vec<String>,
    pub checked_values: BTreeMap<String, bool>,
    elements: Vec<Element>,
}

impl InteractiveDocument {
    /// Re-layout the persistent DOM without replacing input, focus or widget state.
    pub fn resize_screen(&mut self, host: &Viewport, sizing_element: &str) -> Result<(), String> {
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
        viewport.validate()?;
        let source = crate::application::bind(&source, &BTreeMap::new())?;
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
        let mut renderer = NativeRenderer::from_html(&html);
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
        Ok(Self {
            renderer,
            session,
            source,
            viewport,
            last_error: None,
            input_values: BTreeMap::new(),
            activation_requests: Vec::new(),
            file_values: BTreeMap::new(),
            file_requests: Vec::new(),
            checked_values: BTreeMap::new(),
            elements: markup.elements,
        })
    }

    pub fn path_id(&self, path: &str) -> Option<&str> {
        self.elements
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.id.as_str())
    }
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }
    /// Replace compiled CSS without recreating nodes, widget state, input or focus.
    pub fn replace_stylesheet(&mut self, css: &str) -> Result<(), String> {
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
        Ok(())
    }

    pub fn set_input_value(&mut self, path: &str, value: &str) -> Result<(), String> {
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
            .is_some_and(|e| e.text_input_data().is_some());
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

    pub fn dispatch(&mut self, action: Action) -> Result<bool, String> {
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

    fn focusable(&self, scope: Option<&str>) -> Vec<String> {
        self.elements
            .iter()
            .filter(|element| {
                let in_scope =
                    scope.is_some_and(|scope| element.path.starts_with(&format!("{scope}/")));
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
                    ancestor.path != scope.unwrap_or_default()
                        && (element.path == ancestor.path
                            || element.path.starts_with(&format!("{}/", ancestor.path)))
                        && ancestor.attributes.contains_key("hidden")
                });
                in_scope
                    && !disabled
                    && !negative
                    && !hidden
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

    fn patch(&mut self) -> Result<(), String> {
        let mut next = markup::build(&self.source, &self.session)?;
        for element in &mut next.elements {
            if let Some(checked) = self.checked_values.get(&element.path) {
                element
                    .attributes
                    .insert("aria-checked".into(), checked.to_string());
                element.attributes.insert(
                    "data-state".into(),
                    if *checked { "checked" } else { "unchecked" }.into(),
                );
            }
        }
        for element in &next.elements {
            let previous = self
                .elements
                .iter()
                .find(|e| e.path == element.path)
                .ok_or("source shape changed during widget state update")?;
            let node = self
                .renderer
                .document
                .get_element_by_id(&element.id)
                .ok_or("native DOM identity lost")?;
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
                    mutation.set_attribute(node, name(key), value);
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
        self.elements = next.elements;
        let viewport = self.renderer.document.get_viewport();
        self.viewport.width = viewport.window_size.0.max(1);
        self.viewport.height = viewport.window_size.1.max(1);
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

fn name(value: &str) -> blitz_dom::QualName {
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
            self.inputs.insert(path.clone(), input.value.clone());
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
                if [Family::Dialog, Family::AlertDialog].contains(&overlay.family) {
                    state.prevent_default();
                    return;
                }
            }
        }
        if let DomEventData::Click(_) = &event.data
            && let Some(button) = chain.iter().find_map(|id| {
                let node = doc.get_node(*id)?;
                (node.element_data()?.name.local == blitz_dom::local_name!("button"))
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
                .is_some_and(|e| e.name.local == blitz_dom::local_name!("button"))
        {
            self.actions.push(Action::Activate { path });
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
                    && [Family::Dialog, Family::AlertDialog].contains(&widget.family);
                let navigation = widget.items.iter().any(|i| i.path == path)
                    || widget.trigger.as_deref() == Some(&path);
                let control = trap
                    || key_name == "Escape"
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
                        .contains(&key_name.as_str()));
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
        let mut actions = Vec::new();
        let handler = QueuedHandler {
            session: &self.session,
            actions: &mut actions,
            inputs: &mut self.input_values,
        };
        EventDriver::new(&mut self.renderer.document, handler).handle_ui_event(event);
        for action in actions {
            if let Err(error) = self.dispatch(action) {
                self.last_error = Some(error);
            }
        }
    }
}

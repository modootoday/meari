//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use crate::{
    application::markup::{self},
    domain::widgets::Action,
    infrastructure::navigation::PublishedNavigation,
};

impl InteractiveDocument {
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

    pub(in super::super) fn focusable(&self, scope: Option<&str>) -> Vec<String> {
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

    pub(in super::super) fn patch(&mut self) -> Result<(), String> {
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
        let text_updates = super::super::text_patch::prepare(
            &self.renderer.document,
            &self.elements,
            &next.elements,
        )?;
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
                    if !super::super::range_input::is_range(&self.renderer.document, *node) {
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
        super::super::text_patch::apply(&mut self.renderer.document, text_updates);
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

pub(in super::super) fn name(value: &str) -> blitz_dom::QualName {
    blitz_dom::QualName::new(
        None,
        blitz_dom::Namespace::from(""),
        blitz_dom::LocalName::from(value),
    )
}

//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use crate::{
    application::{
        Renderer,
        markup::{self, Element},
    },
    domain::{Viewport, widgets::Session},
    infrastructure::engine::NativeRenderer,
};
use meari::domain::Component;
use std::collections::BTreeMap;

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

    pub(in super::super) fn elements_mut_for_disclosure(
        &mut self,
        id: &str,
    ) -> Option<&mut Element> {
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

    pub(in super::super) fn new_with_optional_assets(
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
            Some(super::super::state_program::mount(source.clone())?)
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
            let text = mutation.create_text_node(&super::super::placeholder_css::project(css));
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

    pub(super) fn from_application_and_assets(
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
            document.state_program =
                Some(super::super::state_program::mount(document.source.clone())?);
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
            let text = mutation.create_text_node(&super::super::placeholder_css::project(css));
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
}

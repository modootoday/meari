use super::super::document::InteractiveDocument;
use crate::domain::{Viewport, widgets::Action};
use meari::{
    application::source_modules::{ModuleDiagnostic, ModuleProgram, ModuleProjection},
    domain::Value,
};
use std::collections::BTreeMap;

fn message(error: ModuleDiagnostic) -> String {
    format!(
        "{}:{}..{}: {}",
        error.file.as_str(),
        error.diagnostic.span.start,
        error.diagnostic.span.end,
        error.diagnostic.message
    )
}

impl InteractiveDocument {
    /// Issue a same-parent movable slot without changing document or callback authority.
    pub fn movable_node_slot(
        &self,
        key: &str,
        reference: meari::domain::source_modules::ComponentReference,
        props: BTreeMap<String, Value>,
    ) -> Result<Value, String> {
        self.module_program
            .as_ref()
            .ok_or("document has no module graph")?
            .node_slots()
            .movable_component(key, reference, props)
    }

    pub fn node_slot(
        &self,
        key: &str,
        reference: meari::domain::source_modules::ComponentReference,
        props: BTreeMap<String, Value>,
    ) -> Result<Value, String> {
        self.module_program
            .as_ref()
            .ok_or("document has no module graph")?
            .node_slots()
            .component(key, reference, props)
    }
    /// The factory receives the mounted root identity before exporting host bindings.
    /// The host retains its single registry and callback owner; the document owns the export.
    pub fn from_module_program_with_callbacks(
        program: ModuleProgram,
        css: &str,
        viewport: Viewport,
        assets: Option<meari::application::assets::VerifiedAssets>,
        source_generation: u64,
        export: impl FnOnce(
            meari::domain::plugins::PluginScope,
        )
            -> Result<meari::application::host_callbacks::CallbackBindings, String>,
    ) -> Result<Self, String> {
        for definition in program.definitions() {
            crate::application::state_program::validate_callbacks(definition, true)?;
        }
        let projection = program.project().map_err(message)?;
        let scope = meari::domain::plugins::PluginScope {
            instance: projection
                .instances
                .get("root")
                .ok_or("module root instance missing")?
                .instance
                .clone(),
            source_generation,
        };
        if source_generation == 0 {
            return Err("callback source generation must be positive".into());
        }
        let mut document =
            Self::new_with_optional_assets(projection.component.clone(), css, viewport, assets)?;
        let mut callbacks = super::super::source_callbacks::CallbackSession::new(
            export(scope.clone())?,
            &scope,
            projection.callback_bindings(),
        )?;
        callbacks.own_nodes(&document)?;
        document.state_inputs = projection.input_bindings();
        document.module_program = Some(program);
        document.callbacks = Some(callbacks);
        Ok(document)
    }

    /// Stage bindings and props before touching the live DOM. A failed preflight keeps
    /// the old export live; a successful commit revokes its queued handles.
    pub fn update_module_props_with_callbacks(
        &mut self,
        props: BTreeMap<String, Value>,
        export: meari::application::host_callbacks::CallbackBindings,
    ) -> Result<(), String> {
        let scope = self
            .callback_scope()
            .ok_or("document callback dispatcher missing")?
            .clone();
        let mut program = self
            .module_program
            .as_ref()
            .ok_or("document has no module graph")?
            .clone();
        let projection = program.update_props(props).map_err(message)?;
        let mut callbacks = super::super::source_callbacks::CallbackSession::new(
            export,
            &scope,
            projection.callback_bindings(),
        )?;
        let inputs = projection.input_bindings();
        crate::application::state_program::retained_shape(
            &self.source.root,
            &projection.component.root,
        )?;
        self.commit_component_projection(projection.component)?;
        if let Err(error) = callbacks.own_nodes(self) {
            self.source_input_poisoned = true;
            return Err(format!("prepared callback node invariant failed: {error}"));
        }
        self.state_inputs = inputs;
        self.module_program = Some(program);
        self.callbacks = Some(callbacks);
        Ok(())
    }
    /// Preserve the current export only for an unchanged graph and range values.
    /// Rebinding callbacks continues to require update_module_props_with_callbacks.
    pub fn update_module_range_props(
        &mut self,
        props: BTreeMap<String, Value>,
    ) -> Result<(), String> {
        self.update_module_range_props_with_text(props, &Default::default())
    }
    /// Existing string text nodes may change only at these explicitly selected paths.
    pub fn update_module_range_props_with_text(
        &mut self,
        props: BTreeMap<String, Value>,
        display_text_paths: &std::collections::BTreeSet<String>,
    ) -> Result<(), String> {
        if self.source_input_poisoned {
            return Err("source input document requires remount after failed recovery".into());
        }
        let mut program = self
            .module_program
            .as_ref()
            .ok_or("document has no module graph")?
            .clone();
        let previous = program.project().map_err(message)?.clone();
        let projection = program.update_props(props).map_err(message)?;
        let callbacks = self
            .callbacks
            .as_ref()
            .ok_or("document callback dispatcher missing")?;
        callbacks.validate_retained_range_props_with_text(
            &previous.component,
            &projection.component,
            &projection.callback_bindings(),
            display_text_paths,
        )?;
        callbacks.validate_owned_nodes(self)?;
        let inputs = projection.input_bindings();
        self.commit_component_projection(projection.component)?;
        if let Err(error) = self
            .callbacks
            .as_ref()
            .expect("retained callback dispatcher")
            .validate_owned_nodes(self)
        {
            self.source_input_poisoned = true;
            self.range_inputs.cancel();
            return Err(error);
        }
        self.state_inputs = inputs;
        self.module_program = Some(program);
        Ok(())
    }
    pub fn from_module_application(
        definition: &meari::application::module_app_bundle::ModuleApplicationDefinition,
        css: &str,
    ) -> Result<Self, String> {
        Self::from_module_application_and_assets(definition, css, None)
    }
    pub fn from_module_application_with_assets(
        definition: &meari::application::module_app_bundle::ModuleApplicationDefinition,
        css: &str,
        assets: meari::application::assets::VerifiedAssets,
    ) -> Result<Self, String> {
        Self::from_module_application_and_assets(definition, css, Some(assets))
    }
    fn from_module_application_and_assets(
        definition: &meari::application::module_app_bundle::ModuleApplicationDefinition,
        css: &str,
        assets: Option<meari::application::assets::VerifiedAssets>,
    ) -> Result<Self, String> {
        definition.validate()?;
        definition
            .application
            .validate_target(meari::domain::build::TargetKind::Native)?;
        let app = &definition.application;
        use std::sync::atomic::{AtomicU64, Ordering};
        static EPOCH: AtomicU64 = AtomicU64::new(1);
        let epoch = EPOCH
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| "native module mount epoch exhausted")?;
        let program =
            definition.program(format!("native-{}", app.stamp.session.as_str()), epoch)?;
        let mut document = Self::from_module_program_and_assets(
            program,
            css,
            app.native_viewport.clone(),
            assets,
        )?;
        document.source_schema = app.source_schema()?;
        document.application = Some(app.clone());
        document.application_generation = app.stamp.generation;
        document.application_definition_revision = 1;
        Ok(document)
    }
    pub fn from_module_program(
        program: ModuleProgram,
        css: &str,
        viewport: Viewport,
    ) -> Result<Self, String> {
        Self::from_module_program_and_assets(program, css, viewport, None)
    }
    pub fn from_module_program_with_assets(
        program: ModuleProgram,
        css: &str,
        viewport: Viewport,
        assets: meari::application::assets::VerifiedAssets,
    ) -> Result<Self, String> {
        Self::from_module_program_and_assets(program, css, viewport, Some(assets))
    }
    fn from_module_program_and_assets(
        program: ModuleProgram,
        css: &str,
        viewport: Viewport,
        assets: Option<meari::application::assets::VerifiedAssets>,
    ) -> Result<Self, String> {
        if program.project().map_err(message)?.uses_hook_document() {
            return Err(
                "prepared hook modules require their owning native host constructor".into(),
            );
        }
        Self::from_module_program_projection(program, css, viewport, assets)
    }

    pub(in crate::infrastructure) fn from_prepared_hook_program(
        program: ModuleProgram,
        css: &str,
        viewport: Viewport,
    ) -> Result<Self, String> {
        Self::from_module_program_projection(program, css, viewport, None)
    }

    fn from_module_program_projection(
        program: ModuleProgram,
        css: &str,
        viewport: Viewport,
        assets: Option<meari::application::assets::VerifiedAssets>,
    ) -> Result<Self, String> {
        for definition in program.definitions() {
            crate::application::state_program::validate(definition)?;
        }
        let projection = program.project().map_err(message)?;
        let mut document =
            Self::new_with_optional_assets(projection.component.clone(), css, viewport, assets)?;
        document.state_inputs = projection.input_bindings();
        document.module_program = Some(program);
        Ok(document)
    }

    pub fn module_projection(&self) -> Result<&ModuleProjection, String> {
        self.module_program
            .as_ref()
            .ok_or("document has no module graph")?
            .project()
            .map_err(message)
    }

    pub fn update_module_props(&mut self, props: BTreeMap<String, Value>) -> Result<(), String> {
        if self.callbacks.is_some() {
            return Err(
                "callback document props require an atomic callback export replacement".into(),
            );
        }
        let mut program = self
            .module_program
            .as_ref()
            .ok_or("document has no module graph")?
            .clone();
        let projection = program.update_props(props).map_err(message)?;
        self.commit_module_projection(program, projection)
    }

    pub(in super::super) fn dispatch_module_event(
        &mut self,
        action: &Action,
    ) -> Result<Option<bool>, String> {
        if self.source_input_poisoned {
            return Err("source input document requires remount after failed recovery".into());
        }
        let Some(program) = self.module_program.as_ref() else {
            return Ok(None);
        };
        let path = match action {
            Action::Activate { path } => path,
            Action::Key { path, key, .. } if key == "Enter" || key == " " => path,
            _ => return Ok(None),
        };
        if !program
            .project()
            .map_err(message)?
            .event_paths()
            .any(|event| event == path)
        {
            return Ok(None);
        }
        let target = self
            .elements()
            .iter()
            .find(|element| element.path == *path)
            .ok_or("module event target missing")?;
        if target.attributes.contains_key("disabled")
            || self.elements().iter().any(|element| {
                (element.path == *path || path.starts_with(&format!("{}/", element.path)))
                    && (element.attributes.contains_key("hidden")
                        || element.attributes.contains_key("inert")
                        || element
                            .attributes
                            .get("aria-disabled")
                            .is_some_and(|value| value == "true"))
            })
        {
            return Ok(Some(false));
        }
        if program.project().map_err(message)?.uses_hook_document() {
            let action = program
                .project()
                .map_err(message)?
                .hook_action(path)
                .ok_or("hook frame handler missing")?;
            self.hook_actions.push(action);
            return Ok(Some(true));
        }
        let mut staged = program.clone();
        let projection = staged.activate(path).map_err(message)?;
        self.commit_module_projection(staged, projection)?;
        Ok(Some(true))
    }

    pub(in super::super) fn dispatch_module_input(
        &mut self,
        path: &str,
        payload: &meari::domain::input_event::InputPayload,
    ) -> Result<bool, String> {
        let Some(program) = self.module_program.as_ref() else {
            return Ok(false);
        };
        let mut staged = program.clone();
        let projection = staged.input(path, payload).map_err(message)?;
        self.commit_module_projection(staged, projection)?;
        Ok(true)
    }

    pub(in super::super) fn commit_module_projection(
        &mut self,
        program: ModuleProgram,
        projection: ModuleProjection,
    ) -> Result<(), String> {
        let inputs = projection.input_bindings();
        self.commit_component_projection(projection.component)?;
        self.state_inputs = inputs;
        self.module_program = Some(program);
        Ok(())
    }
}

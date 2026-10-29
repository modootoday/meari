use super::WebDocument;
use crate::{
    application::{
        dom,
        source_modules::{ModuleDiagnostic, ModuleProgram},
    },
    domain::{Value, source_modules::SourceModuleGraph},
};
use std::collections::BTreeMap;
use web_sys::Element;

fn message(error: ModuleDiagnostic) -> String {
    format!(
        "{}:{}..{}: {}",
        error.file.as_str(),
        error.diagnostic.span.start,
        error.diagnostic.span.end,
        error.diagnostic.message
    )
}

impl super::Inner {
    pub(super) fn dispatch_module_state(&mut self, path: &str) -> Result<bool, String> {
        if self.hook_document.is_some() {
            return self.dispatch_hook_module(path);
        }
        let mut program = self
            .module_program
            .as_ref()
            .ok_or("Web module program missing")?
            .clone();
        let projection = program.activate(path).map_err(message)?;
        let plan = dom::prepare(&projection.component)?;
        if let Err((error, poisoned)) = self.frame.patch(plan, &self.shell) {
            self.poisoned = poisoned;
            return Err(error);
        }
        if let Some(callbacks) = &self.callbacks {
            self.frame.sync_text_controls(&callbacks.text_paths())?;
        }
        self.module_program = Some(program);
        self.last_error = None;
        Ok(true)
    }
}

impl WebDocument {
    pub fn mount_module_application(
        definition: &crate::application::module_app_bundle::ModuleApplicationDefinition,
        css: &str,
    ) -> Result<Self, String> {
        definition.validate()?;
        let app = &definition.application;
        app.validate_target(crate::domain::build::TargetKind::Web)?;
        let body = web_sys::window()
            .ok_or("Window unavailable")?
            .document()
            .ok_or("Document unavailable")?
            .body()
            .ok_or("Body unavailable")?;
        let mut document = Self::mount_modules(
            body.as_ref(),
            definition.modules.clone(),
            app.props.clone(),
            css,
            format!("web-{}", app.stamp.session.as_str()),
            app.stamp.generation,
        )?;
        document.observe_host()?;
        let mut inner = document.inner.borrow_mut();
        inner
            .shell
            .set_attribute("data-meari-session", app.stamp.session.as_str())
            .map_err(super::frame::error)?;
        inner
            .shell
            .set_attribute("data-meari-generation", &app.stamp.generation.to_string())
            .map_err(super::frame::error)?;
        inner.application = Some(app.clone());
        inner.application_generation = app.stamp.generation;
        inner.source_schema = app.source_schema()?;
        drop(inner);
        Ok(document)
    }
    pub fn mount_modules(
        container: &Element,
        graph: SourceModuleGraph,
        props: BTreeMap<String, Value>,
        css: &str,
        document_id: String,
        epoch: u64,
    ) -> Result<Self, String> {
        let program =
            ModuleProgram::mount(graph, props, document_id.clone(), epoch).map_err(message)?;
        Self::mount_module_program(container, program, css, document_id, epoch)
    }
    pub fn mount_module_program(
        container: &Element,
        program: ModuleProgram,
        css: &str,
        document_id: String,
        epoch: u64,
    ) -> Result<Self, String> {
        if program.project().map_err(message)?.uses_hook_document() {
            return Err("prepared hook modules require their owning Web constructor".into());
        }
        for definition in program.definitions() {
            dom::validate_source(definition)?;
        }
        let projection = program.project().map_err(message)?;
        if projection.input_paths().next().is_some() {
            return Err("Web module inputs require the typed property/listener adapter".into());
        }
        let document = Self::mount(
            container,
            &projection.component,
            &BTreeMap::new(),
            css,
            document_id,
            epoch,
        )?;
        document.inner.borrow_mut().module_program = Some(program);
        Ok(document)
    }

    pub fn module_snapshot(
        &self,
    ) -> Result<BTreeMap<String, crate::application::component_state::StateCommit>, String> {
        let inner = self.inner.borrow();
        if inner.closed || inner.poisoned {
            return Err("Web module document is closed or requires recovery".into());
        }
        if inner.hook_document.is_some() {
            return Err("hook roots require the borrowed hook-state reader".into());
        }
        Ok(inner
            .module_program
            .as_ref()
            .ok_or("Web document has no module graph")?
            .project()
            .map_err(message)?
            .instances
            .clone())
    }

    pub fn update_module_props(&mut self, props: BTreeMap<String, Value>) -> Result<(), String> {
        let mut inner = self.inner.borrow_mut();
        if inner.hook_document.is_some() {
            return inner.update_hook_props(props);
        }
        if inner.callbacks.is_some() {
            return Err(
                "Web callback document props require an atomic callback export replacement".into(),
            );
        }
        if inner.closed || inner.poisoned {
            return Err("Web module document is closed or requires recovery".into());
        }
        let mut program = inner
            .module_program
            .as_ref()
            .ok_or("Web document has no module graph")?
            .clone();
        let projection = program.update_props(props).map_err(message)?;
        let plan = dom::prepare(&projection.component)?;
        let shell = inner.shell.clone();
        if let Err((error, poisoned)) = inner.frame.patch(plan, &shell) {
            inner.poisoned = poisoned;
            inner.last_error = Some(error.clone());
            return Err(error);
        }
        inner.module_program = Some(program);
        inner.last_error = None;
        Ok(())
    }
}

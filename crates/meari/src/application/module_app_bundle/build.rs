//! Module-aware bundle loading; single-source HMR must never discard child schemas.
use super::ModuleApplicationDefinition;
use crate::{
    application::builder::BuildBundle,
    domain::build::{BuildPath, InputRole},
};

impl BuildBundle {
    pub fn module_application_definition(
        &self,
    ) -> Result<Option<ModuleApplicationDefinition>, String> {
        self.validate()?;
        let path = BuildPath::try_from("runtime/module-application.json".to_owned())?;
        let Some(bytes) = self.files.get(&path) else {
            return Ok(None);
        };
        let definition: ModuleApplicationDefinition =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        definition.validate()?;
        definition.application.validate_manifest(&self.manifest)?;
        if definition.modules.entry.module != self.manifest.graph.entry {
            return Err("module entry differs from bundle input graph".into());
        }
        let inputs = self
            .manifest
            .graph
            .files
            .iter()
            .filter(|(_, input)| input.role == InputRole::Component);
        let mut count = 0;
        for (path, input) in inputs {
            count += 1;
            let module = definition
                .modules
                .modules
                .get(path)
                .ok_or("bundle component missing from module definition")?;
            if module.content != input.content || module.source_bytes != input.length {
                return Err("module source identity differs from bundle input".into());
            }
        }
        if count != definition.modules.modules.len() {
            return Err("module definition contains undeclared component inputs".into());
        }
        let app_path = BuildPath::try_from("runtime/application.json".to_owned())?;
        let app: crate::application::app_bundle::ApplicationDefinition = serde_json::from_slice(
            self.files
                .get(&app_path)
                .ok_or("runtime application missing")?,
        )
        .map_err(|error| error.to_string())?;
        if serde_json::to_value(app).map_err(|error| error.to_string())?
            != serde_json::to_value(&definition.application).map_err(|error| error.to_string())?
        {
            return Err("module application differs from bundle application".into());
        }
        Ok(Some(definition))
    }
}

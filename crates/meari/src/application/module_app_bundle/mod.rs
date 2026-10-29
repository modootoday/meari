//! Graph applications retain the existing single-source definition without adding fields to it.
use super::{app_bundle::ApplicationDefinition, source_modules::ModuleProgram};
use crate::domain::source_modules::SourceModuleGraph;
use serde::{Deserialize, Serialize};
mod build;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleApplicationDefinition {
    pub schema_version: u32,
    pub application: ApplicationDefinition,
    pub modules: SourceModuleGraph,
}

impl ModuleApplicationDefinition {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("unsupported module application schema".into());
        }
        self.application.validate()?;
        self.modules.validate()?;
        let entry = self.modules.definition(&self.modules.entry)?;
        if entry.component != self.application.component {
            return Err("module application entry differs from its declared application".into());
        }
        self.program("module-application-validation".into(), 1)
            .map(|_| ())
    }

    pub fn program(&self, document: String, epoch: u64) -> Result<ModuleProgram, String> {
        if self.schema_version != 1 {
            return Err("unsupported module application schema".into());
        }
        self.application.validate()?;
        if self.modules.definition(&self.modules.entry)?.component != self.application.component {
            return Err("module application entry differs from its declared application".into());
        }
        ModuleProgram::mount(
            self.modules.clone(),
            self.application.props.clone(),
            document,
            epoch,
        )
        .map_err(|error| {
            format!(
                "{}:{}..{}: {}",
                error.file.as_str(),
                error.diagnostic.span.start,
                error.diagnostic.span.end,
                error.diagnostic.message
            )
        })
    }
}

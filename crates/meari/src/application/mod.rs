pub use self::bundling::app_bundle;
use crate::domain::{Component, Diagnostic, Span, validation::validate};
use serde::{Deserialize, Serialize};
pub mod assets;
pub mod binding;
pub mod builder;
pub mod component_state;
pub mod dom;
pub use self::bundling::hmr;
pub mod hook_values;
pub mod host_callbacks;
pub mod icons;
pub use self::environment::media;
pub mod module_app_bundle;
pub mod plugins;
pub(crate) mod radio_group;
pub mod range_parts;
pub mod repeat;
pub use self::environment::responsive;
pub mod screen;
pub mod source_modules;
pub mod state_program;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    Tsx,
}

#[derive(Debug, Serialize)]
pub struct Mapping {
    pub source: Span,
    pub generated: Span,
}

#[derive(Debug, Serialize)]
pub struct Output {
    pub schema_version: u32,
    pub code: String,
    pub mappings: Vec<Mapping>,
    pub design_system: crate::domain::design::Capabilities,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRequest {
    pub source: String,
    pub language: Language,
    pub props: std::collections::BTreeMap<String, crate::domain::Value>,
    pub theme: crate::domain::design::theme::Theme,
    pub environment: crate::domain::design::theme::Environment,
    #[serde(default)]
    pub interactions: std::collections::BTreeMap<String, crate::domain::design::theme::Interaction>,
}

pub trait Frontend {
    fn parse(&self, source: &str, language: Language) -> Result<Component, Diagnostic>;
}

/// Native preparation shares the component and design contracts with source conversion.
pub fn prepare_native(
    component: &Component,
    context: &crate::domain::design::native::Context<'_>,
) -> Result<crate::domain::design::native::NativePlan, Diagnostic> {
    crate::domain::design::native::prepare(component, context)
}

pub trait Backend {
    fn emit(&self, component: &Component, language: Language) -> Output;
}

/// The use case depends on contracts, never a concrete parser or renderer.
pub struct Converter<'a> {
    frontend: &'a dyn Frontend,
    backend: &'a dyn Backend,
}

impl<'a> Converter<'a> {
    pub const fn new(frontend: &'a dyn Frontend, backend: &'a dyn Backend) -> Self {
        Self { frontend, backend }
    }
    pub fn parse(&self, source: &str, language: Language) -> Result<Component, Diagnostic> {
        let component = self.frontend.parse(source, language)?;
        validate(&component)?;
        Ok(component)
    }
    pub fn emit(&self, component: &Component, language: Language) -> Result<Output, Diagnostic> {
        validate(component)?;
        if crate::domain::host_callbacks::contains_runtime(&component.root) {
            return Err(Diagnostic::unsupported(
                component.root.span,
                "bound runtime callback identities cannot be emitted as authoring source",
            ));
        }
        Ok(self.backend.emit(component, language))
    }
    pub fn convert(
        &self,
        source: &str,
        from: Language,
        to: Language,
    ) -> Result<Output, Diagnostic> {
        self.emit(&self.parse(source, from)?, to)
    }
    pub fn prepare_native(
        &self,
        request: &NativeRequest,
    ) -> Result<crate::domain::design::native::NativePlan, Diagnostic> {
        let component = self.parse(&request.source, request.language)?;
        prepare_native(
            &component,
            &crate::domain::design::native::Context {
                props: &request.props,
                theme: &request.theme,
                environment: request.environment,
                interactions: &request.interactions,
            },
        )
    }
}
mod bundling;
mod environment;
pub mod hook_document;
pub mod hook_expression;

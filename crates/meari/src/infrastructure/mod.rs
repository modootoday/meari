#[cfg(not(target_arch = "wasm32"))]
pub mod artifact_store;
pub(crate) mod generate;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod rust;
pub mod source_callbacks;
pub(crate) mod tsx;
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
pub mod web_document;
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
pub mod web_host;
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
pub mod web_media;

use crate::{
    application::{Backend, Frontend, Language, Output},
    domain::{Component, Diagnostic},
};

pub struct SourceAdapters;
impl crate::application::source_modules::ModuleFrontend for SourceAdapters {
    fn parse_module(
        &self,
        source: &str,
    ) -> Result<crate::application::source_modules::ParsedSourceModule, Diagnostic> {
        tsx::parse_module(source)
    }
}
impl Frontend for SourceAdapters {
    fn parse(&self, source: &str, language: Language) -> Result<Component, Diagnostic> {
        match language {
            Language::Rust => rust::parse(source),
            Language::Tsx => tsx::parse(source),
        }
    }
}
impl Backend for SourceAdapters {
    fn emit(&self, component: &Component, language: Language) -> Output {
        generate::emit(component, language)
    }
}

pub(crate) mod generate;
pub(crate) mod rust;
pub(crate) mod tsx;
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
pub mod web_media;

use crate::{
    application::{Backend, Frontend, Language, Output},
    domain::{Component, Diagnostic},
};

pub struct SourceAdapters;
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

//! Portable, deliberately bounded Rust/TSX component conversion.
pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod runtime;
#[cfg(feature = "wasm")]
pub mod wasm;

use application::{Converter, Language, Output};
use domain::{Component, Diagnostic};
use infrastructure::SourceAdapters;

const CONVERTER: Converter<'static> = Converter::new(&SourceAdapters, &SourceAdapters);

pub fn parse(source: &str, language: Language) -> Result<Component, Diagnostic> {
    CONVERTER.parse(source, language)
}
pub fn emit(component: &Component, language: Language) -> Result<Output, Diagnostic> {
    CONVERTER.emit(component, language)
}
pub fn convert(source: &str, from: Language, to: Language) -> Result<Output, Diagnostic> {
    CONVERTER.convert(source, from, to)
}

pub use application::NativeRequest;

pub fn prepare_native(
    request: &NativeRequest,
) -> Result<domain::design::native::NativePlan, Diagnostic> {
    CONVERTER.prepare_native(request)
}

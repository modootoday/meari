//! WebAssembly transport over the shared conversion use case.
use crate::{
    application::Language,
    domain::{Diagnostic, Span},
};
use wasm_bindgen::prelude::*;

fn language(value: &str) -> Result<Language, Diagnostic> {
    match value {
        "rust" => Ok(Language::Rust),
        "tsx" => Ok(Language::Tsx),
        _ => Err(Diagnostic {
            code: "input".into(),
            message: "language must be rust or tsx".into(),
            span: Span::default(),
        }),
    }
}

/// Returns schema-versioned Output JSON; throws Diagnostic JSON on failure.
#[wasm_bindgen]
pub fn convert_source(source: &str, from: &str, to: &str) -> Result<String, JsValue> {
    let result = language(from)
        .and_then(|from| language(to).and_then(|to| crate::convert(source, from, to)));
    match result {
        Ok(output) => Ok(serde_json::to_string(&output).expect("output serialization")),
        Err(error) => Err(JsValue::from_str(
            &serde_json::to_string(&error).expect("diagnostic serialization"),
        )),
    }
}

/// Shared typed native preparation, independent of a browser or native renderer.
#[wasm_bindgen]
pub fn prepare_native_plan(request_json: &str) -> Result<String, JsValue> {
    let request = serde_json::from_str(request_json).map_err(|error| Diagnostic {
        code: "input".into(),
        message: error.to_string(),
        span: Span::default(),
    });
    match request.and_then(|request| crate::prepare_native(&request)) {
        Ok(plan) => Ok(serde_json::to_string(&plan).expect("native plan serialization")),
        Err(error) => Err(JsValue::from_str(
            &serde_json::to_string(&error).expect("diagnostic serialization"),
        )),
    }
}

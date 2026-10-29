//! Default design vocabulary. Source preservation and native capability are separate.
pub mod components;
pub mod fonts;
pub mod icons;
pub mod native;
pub mod stylesheet;
pub mod theme;
pub mod utilities;

use crate::domain::{Component, Diagnostic, Node, NodeKind, Value};
use serde::Serialize;

pub const PROFILE: &str = "tailwind-shadcn-v1";

#[derive(Clone, Debug, Serialize)]
pub struct Capabilities {
    pub profile: &'static str,
    pub native_syntax_supported: bool,
    pub native_renderer_verified: bool,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn diagnostic(node: &Node, code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        message: message.into(),
        span: node.span,
    }
}

/// Never rewrites or discards unsupported source expressions.
pub fn analyze(component: &Component) -> Capabilities {
    fn visit(node: &Node, issues: &mut Vec<Diagnostic>) {
        if let NodeKind::Text(Value::Repeat(repeat)) = &node.kind {
            visit(&repeat.template, issues);
        }
        if let NodeKind::Element {
            tag,
            attributes,
            children,
        } = &node.kind
        {
            if let Err(error) = components::validate(node, tag, attributes, children) {
                issues.push(error);
            }
            if let Some(class) = attributes.iter().find(|a| a.name == "className") {
                match &class.value {
                    Value::String(classes) => {
                        if let Err(message) = utilities::parse(classes) {
                            issues.push(diagnostic(node, "native-style", message));
                        }
                    }
                    _ => issues.push(diagnostic(node, "native-style", "className must be static for native preparation; original binding is preserved")),
                }
            }
            for child in children {
                visit(child, issues);
            }
        }
    }
    let mut diagnostics = Vec::new();
    visit(&component.root, &mut diagnostics);
    Capabilities {
        profile: PROFILE,
        native_syntax_supported: diagnostics.is_empty(),
        native_renderer_verified: false,
        diagnostics,
    }
}

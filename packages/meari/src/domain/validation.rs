use crate::domain::*;
use std::collections::HashSet;

fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && ![
            "type",
            "fn",
            "pub",
            "struct",
            "return",
            "self",
            "Self",
            "true",
            "false",
            "let",
            "const",
            "default",
            "class",
            "new",
            "in",
            "ref",
            "match",
            "async",
            "await",
            "move",
            "use",
            "mod",
            "crate",
            "super",
            "loop",
            "while",
            "for",
            "if",
            "else",
            "impl",
            "trait",
            "enum",
            "dyn",
            "as",
            "break",
            "continue",
            "where",
            "static",
            "unsafe",
            "extern",
            "yield",
            "try",
            "do",
            "var",
            "function",
            "export",
            "import",
            "delete",
            "void",
            "switch",
            "case",
            "throw",
            "catch",
            "finally",
            "with",
            "extends",
            "instanceof",
            "typeof",
            "this",
            "null",
            "mut",
            "gen",
            "abstract",
            "become",
            "box",
            "final",
            "macro",
            "override",
            "priv",
            "unsized",
            "virtual",
            "implements",
            "interface",
            "package",
            "private",
            "protected",
            "public",
            "eval",
            "arguments",
        ]
        .contains(&name)
}

pub fn validate(component: &Component) -> Result<(), Diagnostic> {
    let err = |message| Diagnostic::unsupported(component.root.span, message);
    if !matches!(component.root.kind, NodeKind::Element { .. }) {
        return Err(err("element root required"));
    }
    if !identifier(&component.name)
        || !component.name.starts_with(|c: char| c.is_ascii_uppercase())
        || component.name == "Props"
        || (component.client && component.name == "BRIDGE_USE_CLIENT")
    {
        return Err(err(
            "component requires a portable PascalCase identifier other than Props",
        ));
    }
    let mut names = HashSet::new();
    for name in crate::domain::hooks::ids(&component.root) {
        if !identifier(&name)
            || ["props", "useId", "Props"].contains(&name.as_str())
            || name == component.name
            || crate::domain::design::components::module(&name).is_some()
        {
            return Err(err("invalid or shadowing useId binding"));
        }
    }
    for prop in &component.props {
        if !identifier(&prop.name) || !names.insert(&prop.name) {
            return Err(err("invalid or duplicate prop"));
        }
    }
    fn value_type(
        value: &Value,
        component: &Component,
        span: Span,
    ) -> Result<PropType, Diagnostic> {
        match value {
            Value::String(_) => Ok(PropType::String),
            Value::Id(_) => Ok(PropType::String),
            Value::Bool(_) => Ok(PropType::Bool),
            Value::Prop(name) => component
                .props
                .iter()
                .find(|p| p.name == *name)
                .map(|p| p.kind)
                .ok_or_else(|| Diagnostic::unsupported(span, format!("unknown prop {name}"))),
        }
    }
    fn node(node: &Node, component: &Component) -> Result<(), Diagnostic> {
        let err = |message| Diagnostic::unsupported(node.span, message);
        match &node.kind {
            NodeKind::Text(value) => {
                if value_type(value, component, node.span)? != PropType::String {
                    return Err(err("text requires string"));
                }
            }
            NodeKind::Element {
                tag,
                attributes,
                children,
            } => {
                if !identifier(tag) {
                    return Err(err(
                        "only simple intrinsic/custom component names are supported",
                    ));
                }
                let mut names = HashSet::new();
                if tag == &component.name
                    && crate::domain::design::components::module(tag).is_some()
                {
                    return Err(err(
                        "component name shadows a registered design-system dependency",
                    ));
                }
                for attribute in attributes {
                    let name = &attribute.name;
                    if name.starts_with("on")
                        || ["key", "ref", "children", "dangerouslySetInnerHTML"]
                            .contains(&name.as_str())
                    {
                        return Err(err(
                            "events and React special attributes require a future explicit contract",
                        ));
                    }
                    if !name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                        || name.is_empty()
                        || !name.starts_with(|c: char| c.is_ascii_alphabetic())
                        || !names.insert(name)
                    {
                        return Err(err("invalid or duplicate attribute"));
                    }
                    value_type(&attribute.value, component, node.span)?;
                }
                for child in children {
                    validate_node(child, component)?;
                }
            }
        }
        Ok(())
    }
    // Keep traversal local to validation; no renderer or platform dependency.
    fn validate_node(n: &Node, c: &Component) -> Result<(), Diagnostic> {
        node(n, c)
    }
    node(&component.root, component)
}

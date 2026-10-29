//! Portable DOM plans. Browser handles and side effects belong to the adapter.
use crate::domain::{Component, Node, NodeKind, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomNode {
    Text(String),
    Element {
        tag: String,
        attributes: BTreeMap<String, String>,
        children: Vec<DomNode>,
    },
}

fn tag(name: &str) -> Result<&str, String> {
    match name {
        "Button" => Ok("button"),
        "Input" => Ok("input"),
        "Label" => Ok("label"),
        "a" | "abbr" | "address" | "article" | "aside" | "audio" | "b" | "blockquote" | "br"
        | "button" | "caption" | "code" | "col" | "colgroup" | "dd" | "del" | "details" | "div"
        | "dl" | "dt" | "em" | "fieldset" | "figcaption" | "figure" | "footer" | "form" | "h1"
        | "h2" | "h3" | "h4" | "h5" | "h6" | "header" | "hr" | "i" | "img" | "input" | "kbd"
        | "label" | "legend" | "li" | "main" | "mark" | "nav" | "ol" | "optgroup" | "option"
        | "output" | "p" | "picture" | "pre" | "progress" | "s" | "section" | "select"
        | "small" | "source" | "span" | "strong" | "sub" | "summary" | "sup" | "table"
        | "tbody" | "td" | "textarea" | "th" | "thead" | "time" | "tr" | "u" | "ul" | "video" => {
            Ok(name)
        }
        _ => Err(format!("Web DOM adapter is not registered for {name}")),
    }
}

fn boolean_attribute(name: &str) -> bool {
    matches!(
        name,
        "disabled"
            | "hidden"
            | "inert"
            | "checked"
            | "multiple"
            | "required"
            | "readonly"
            | "autofocus"
            | "selected"
            | "open"
            | "controls"
            | "loop"
            | "muted"
    )
}

fn safe_url(value: &str) -> bool {
    let value = value.trim();
    if value.chars().any(char::is_control) {
        return false;
    }
    let head = value.split(['/', '?', '#']).next().unwrap_or_default();
    head.split_once(':').is_none_or(|(scheme, _)| {
        ["http", "https", "mailto", "tel"].contains(&scheme.to_ascii_lowercase().as_str())
    })
}

pub fn prepare(component: &Component) -> Result<DomNode, String> {
    if !component.props.is_empty()
        || !component.states.is_empty()
        || !component.hook_order.is_empty()
    {
        return Err("DOM plan requires a bound runtime projection".into());
    }
    fn visit(node: &Node) -> Result<DomNode, String> {
        match &node.kind {
            NodeKind::Text(Value::String(value)) => Ok(DomNode::Text(value.clone())),
            NodeKind::Text(_) => Err("DOM text requires a resolved string".into()),
            NodeKind::Element {
                tag: name,
                attributes,
                children,
            } => {
                let tag = tag(name)?.to_owned();
                let primitive_component = ["Button", "Input", "Label"].contains(&name.as_str());
                if ["br", "hr", "img", "input", "source", "col"].contains(&tag.as_str())
                    && !children.is_empty()
                {
                    return Err(format!("void DOM element cannot contain children: {tag}"));
                }
                let mut out = BTreeMap::new();
                for attribute in attributes {
                    let name = match attribute.name.as_str() {
                        "className" => "class",
                        "htmlFor" => "for",
                        "defaultValue" => "value",
                        "defaultChecked" => "checked",
                        name => name,
                    }
                    .to_ascii_lowercase();
                    if name.starts_with("on")
                        || name == "srcdoc"
                        || name == "dangerouslysetinnerhtml"
                        || name.is_empty()
                        || !name
                            .chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
                    {
                        return Err(format!("unsafe Web DOM attribute: {name}"));
                    }
                    if primitive_component
                        && ["variant", "size", "aschild"].contains(&name.as_str())
                    {
                        return Err(format!(
                            "Web component recipe requires an explicit adapter: {name}"
                        ));
                    }
                    let value = match &attribute.value {
                        Value::String(value) => value.clone(),
                        Value::Bool(false) if boolean_attribute(&name) => continue,
                        Value::Bool(true) if boolean_attribute(&name) => String::new(),
                        Value::Bool(value) => value.to_string(),
                        _ => return Err(format!("unresolved DOM attribute: {name}")),
                    };
                    if ["href", "src", "action", "formaction", "poster"].contains(&name.as_str())
                        && !safe_url(&value)
                    {
                        return Err(format!("unsupported Web URL scheme in {name}"));
                    }
                    if out.insert(name.clone(), value).is_some() {
                        return Err(format!("duplicate normalized DOM attribute: {name}"));
                    }
                }
                if primitive_component {
                    let classes =
                        crate::domain::design::components::recipe(name, "default", "default");
                    let custom = out.remove("class").unwrap_or_default();
                    out.insert(
                        "class".into(),
                        format!("{} {}", classes.trim(), custom).trim().to_owned(),
                    );
                }
                Ok(DomNode::Element {
                    tag,
                    attributes: out,
                    children: children.iter().map(visit).collect::<Result<_, _>>()?,
                })
            }
        }
    }
    visit(&super::radio_group::prepare(component)?.root)
}

pub fn validate_source(component: &Component) -> Result<(), String> {
    validate_source_callbacks(component, false)
}

pub(crate) fn validate_source_callbacks(
    component: &Component,
    callbacks: bool,
) -> Result<(), String> {
    fn visit(node: &Node, callbacks: bool) -> Result<(), String> {
        if let NodeKind::Text(Value::NodeSlot(crate::domain::node_slots::NodeSlot::Content(
            nodes,
        ))) = &node.kind
        {
            for child in nodes {
                visit(child, callbacks)?;
            }
        }
        if let NodeKind::Text(Value::Repeat(repeat)) = &node.kind {
            return visit(&repeat.template, callbacks);
        }
        if let NodeKind::Element {
            tag,
            attributes,
            children,
            ..
        } = &node.kind
        {
            for attribute in attributes {
                let callback_source = match &attribute.value {
                    Value::Callback(handler) => Some(handler.source),
                    Value::BoundCallback(handler) => Some(handler.source),
                    _ => None,
                };
                if callback_source
                    == Some(crate::domain::host_callbacks::CallbackSource::BooleanValue)
                {
                    return Err(
                        "Web Boolean callbacks require the controlled widget adapter".into(),
                    );
                }
                if let Value::NodeSlot(crate::domain::node_slots::NodeSlot::Content(nodes)) =
                    &attribute.value
                {
                    for child in nodes {
                        visit(child, callbacks)?;
                    }
                }
                if matches!(
                    attribute.value,
                    Value::Callback(_) | Value::BoundCallback(_)
                ) && !callbacks
                {
                    return Err(
                        "Web host callback events require the scoped callback listener adapter"
                            .into(),
                    );
                }
                if matches!(attribute.value, Value::Input(_)) {
                    return Err(
                        "Web source input events require the typed input listener adapter".into(),
                    );
                }
                if ["input", "textarea", "select", "Input"].contains(&tag.as_str())
                    && ["value", "checked"].contains(&attribute.name.as_str())
                    && !(callbacks
                        && attribute.name == "value"
                        && ["input", "textarea", "Input"].contains(&tag.as_str()))
                {
                    return Err(format!(
                        "Web controlled {} requires a controlled-input adapter; use defaultValue/defaultChecked for uncontrolled input",
                        attribute.name
                    ));
                }
                if matches!(attribute.value, Value::State(_))
                    && ["value", "checked", "open"].contains(&attribute.name.as_str())
                    && !(callbacks
                        && attribute.name == "value"
                        && ["input", "textarea", "Input"].contains(&tag.as_str()))
                {
                    return Err(format!(
                        "dynamic Web {} requires controlled callbacks",
                        attribute.name
                    ));
                }
            }
            for child in children {
                visit(child, callbacks)?;
            }
        }
        Ok(())
    }
    visit(&component.root, callbacks)
}

#[cfg(test)]
mod boolean_admission_tests {
    #[test]
    fn unsupported_boolean_source_and_bound_handlers_are_explicit() {
        use crate::{
            application::{Language, binding},
            domain::{
                Value,
                host_callbacks::{CallbackReference, CallbackSignature},
            },
        };
        use std::collections::BTreeMap;
        let source = "type Props={onCheckedChange:(value:boolean)=>void};export function Panel(props:Props){return <Switch onCheckedChange={value=>props.onCheckedChange(value)}/>;}";
        let component = crate::parse(source, Language::Tsx).unwrap();
        let bound = binding::bind(
            &component,
            &BTreeMap::from([(
                "onCheckedChange".into(),
                Value::CallbackReference(CallbackReference {
                    binding: "checked".into(),
                    signature: CallbackSignature::Boolean,
                }),
            )]),
        )
        .unwrap();
        for definition in [&component, &bound] {
            assert!(
                super::validate_source_callbacks(definition, true)
                    .unwrap_err()
                    .contains("Web Boolean callbacks require")
            );
        }
    }
}

//! Resolve consumer props and bounded document IDs before runtime projection.
use crate::domain::{Component, Node, NodeKind, Value};
use std::collections::BTreeMap;

pub fn bind(component: &Component, props: &BTreeMap<String, Value>) -> Result<Component, String> {
    bind_with_scope(component, props, None)
}

/// A host sharing one DOM across mounts supplies a unique stable document scope.
pub fn bind_with_scope(
    component: &Component,
    props: &BTreeMap<String, Value>,
    scope: Option<&str>,
) -> Result<Component, String> {
    crate::domain::validation::validate(component).map_err(|error| error.message)?;
    if props.len() != component.props.len() {
        return Err("exact declared bindings required".into());
    }
    for prop in &component.props {
        let valid = props
            .get(&prop.name)
            .is_some_and(|value| prop.kind.accepts(value));
        if !valid {
            return Err(format!("invalid binding: {}", prop.name));
        }
    }
    fn visit(
        node: &mut Node,
        props: &BTreeMap<String, Value>,
        scope: Option<&str>,
        depth: usize,
        count: &mut usize,
    ) -> Result<(), String> {
        *count += 1;
        if depth > 128 || *count > 100000 {
            return Err("slot binding cycle or node/depth budget exceeded".into());
        }
        let resolve = |value: &mut Value| -> Result<(), String> {
            match value {
                Value::Predicate(expression) => {
                    *value = if expression.contains_state() || expression.contains_local() {
                        Value::Predicate(Box::new(
                            expression
                                .bind_props(props)
                                .map_err(|error| error.message)?,
                        ))
                    } else {
                        Value::Bool(expression.evaluate(props).map_err(|error| error.message)?)
                    };
                }
                Value::NotProp(name) => {
                    let Value::Bool(value_) = props[name] else {
                        unreachable!("validated boolean binding");
                    };
                    *value = Value::Bool(!value_);
                }
                Value::Callback(handler) => {
                    let Value::CallbackReference(reference) = &props[&handler.prop] else {
                        unreachable!("validated callback binding");
                    };
                    *value = Value::BoundCallback(crate::domain::host_callbacks::BoundCallback {
                        reference: reference.clone(),
                        source: handler.source,
                        span: handler.span,
                        captured_text: handler.capture_prop.as_ref().map(|name| {
                            let Value::String(value) = &props[name] else {
                                unreachable!("validated string capture binding");
                            };
                            value.clone()
                        }),
                    });
                }
                Value::Prop(name) => *value = props[name].clone(),
                Value::Member(member) => {
                    if let Value::Prop(name) = member.root.as_ref() {
                        *member.root = props
                            .get(name)
                            .ok_or("record member prop was not bound")?
                            .clone();
                    }
                    if let Some(resolved) = member.bound()? {
                        *value = resolved;
                    }
                }
                Value::Id(name) => {
                    *value = Value::String(match scope {
                        Some(scope) => format!("meari-{scope}-{name}"),
                        None => crate::domain::hooks::stable_id(name),
                    })
                }
                _ => {}
            };
            Ok(())
        };
        match &mut node.kind {
            NodeKind::Text(Value::Repeat(repeat)) => {
                resolve(&mut repeat.source)?;
                visit(&mut repeat.template, props, scope, depth + 1, count)?;
            }
            NodeKind::Text(value) => {
                resolve(value)?;
                if let Value::NodeSlot(crate::domain::node_slots::NodeSlot::Content(nodes)) = value
                {
                    for child in nodes {
                        visit(child, props, scope, depth + 1, count)?;
                    }
                }
            }
            NodeKind::Element {
                attributes,
                children,
                ..
            } => {
                for attribute in attributes {
                    resolve(&mut attribute.value)?;
                    if let Value::NodeSlot(crate::domain::node_slots::NodeSlot::Content(nodes)) =
                        &mut attribute.value
                    {
                        for child in nodes {
                            visit(child, props, scope, depth + 1, count)?;
                        }
                    }
                }
                for child in children {
                    visit(child, props, scope, depth + 1, count)?;
                }
            }
        }
        Ok(())
    }
    let mut bound = component.clone();
    if scope
        .is_some_and(|scope| scope.is_empty() || !scope.chars().all(|ch| ch.is_ascii_hexdigit()))
    {
        return Err("runtime ID scope must be nonempty hexadecimal".into());
    }
    visit(&mut bound.root, props, scope, 0, &mut 0)?;
    super::repeat::expand(&mut bound, scope)?;
    bound.props.clear();
    bound.hook_order.clear();
    Ok(bound)
}

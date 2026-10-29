//! A primitive and its single compatible child share one rendered element.
use meari::domain::{Attribute, Node, NodeKind, Value};
use std::collections::BTreeMap;
#[cfg(test)]
mod tests;

pub(super) struct Composition {
    pub node: Node,
    pub child: Node,
    pub child_path: String,
}

pub(crate) fn validate_handlers(node: &Node) -> Result<(), String> {
    let NodeKind::Element {
        attributes,
        children,
        ..
    } = &node.kind
    else {
        return Ok(());
    };
    if !attributes
        .iter()
        .any(|attribute| attribute.name == "asChild" && attribute.value != Value::Bool(false))
    {
        return Ok(());
    }
    let child_attributes = children
        .first()
        .and_then(|child| match &child.kind {
            NodeKind::Element { attributes, .. } => Some(attributes.as_slice()),
            _ => None,
        })
        .unwrap_or_default();
    if attributes
        .iter()
        .chain(child_attributes)
        .any(|attribute| attribute.name.starts_with("on"))
    {
        return Err("native asChild handlers require atomic composed event admission".into());
    }
    Ok(())
}

pub(super) fn prepare(node: &Node, path: &str) -> Result<Option<Composition>, String> {
    let NodeKind::Element {
        tag,
        attributes,
        children,
    } = &node.kind
    else {
        return Ok(None);
    };
    match attributes
        .iter()
        .find(|attribute| attribute.name == "asChild")
        .map(|attribute| &attribute.value)
    {
        None | Some(Value::Bool(false)) => return Ok(None),
        Some(Value::Bool(true)) => {}
        _ => return Err(format!("native {tag}.asChild requires bound bool")),
    };
    if !matches!(
        tag.as_str(),
        "DialogTrigger"
            | "DialogClose"
            | "AlertDialogTrigger"
            | "AlertDialogCancel"
            | "AlertDialogAction"
    ) {
        return Err(format!(
            "native {tag}.asChild requires its semantic adapter"
        ));
    }
    validate_handlers(node)?;
    let [child] = children.as_slice() else {
        return Err(format!(
            "native {tag}.asChild requires exactly one element child"
        ));
    };
    let NodeKind::Element {
        tag: child_tag,
        attributes: child_attributes,
        children: child_children,
    } = &child.kind
    else {
        return Err(format!("native {tag}.asChild requires an element child"));
    };
    if !matches!(child_tag.as_str(), "Button" | "button") {
        return Err(format!(
            "native {tag}.asChild child {child_tag} requires a compatible control adapter"
        ));
    }
    if child_attributes
        .iter()
        .any(|attribute| attribute.name == "asChild" && attribute.value != Value::Bool(false))
    {
        return Err("native nested asChild requires an explicit composition chain".into());
    }
    let mut merged = attributes
        .iter()
        .filter(|attribute| attribute.name != "asChild")
        .map(|attribute| (attribute.name.clone(), attribute.value.clone()))
        .collect::<BTreeMap<_, _>>();
    for attribute in child_attributes
        .iter()
        .filter(|attribute| attribute.name != "asChild")
    {
        if attribute.name == "className"
            && let Some(previous) = merged.get("className")
        {
            let (Value::String(before), Value::String(after)) = (previous, &attribute.value) else {
                return Err("native asChild className requires bound string".into());
            };
            merged.insert(
                attribute.name.clone(),
                Value::String(format!("{before} {after}")),
            );
        } else if matches!(attribute.name.as_str(), "disabled" | "hidden" | "inert")
            && merged.get(&attribute.name) == Some(&Value::Bool(true))
        {
            // A child cannot silently remove the primitive's admission boundary.
        } else if attribute.name == "aria-disabled"
            && merged.get(&attribute.name).is_some_and(|value| {
                matches!(value, Value::Bool(true))
                    || matches!(value,Value::String(value) if value == "true")
            })
        {
            // Keep DOM accessibility and the primitive's disabled policy consistent.
        } else {
            merged.insert(attribute.name.clone(), attribute.value.clone());
        }
    }
    Ok(Some(Composition {
        node: Node {
            kind: NodeKind::Element {
                tag: tag.clone(),
                attributes: merged
                    .into_iter()
                    .map(|(name, value)| Attribute { name, value })
                    .collect(),
                children: child_children.clone(),
            },
            span: node.span,
        },
        child: child.clone(),
        child_path: format!("{path}/0"),
    }))
}

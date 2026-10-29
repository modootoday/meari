//! Expand validated source repeats only in the bound runtime projection.
use crate::domain::{Attribute, Component, Node, NodeKind, Value, build::ContentHash};
use std::collections::BTreeSet;

pub const IDENTITY_ATTRIBUTE: &str = "data-meari-repeat";

pub(super) fn expand(component: &mut Component, scope: Option<&str>) -> Result<(), String> {
    fn slot(
        value: &mut Value,
        scope: &str,
        path: &str,
        count: &mut usize,
        depth: usize,
    ) -> Result<(), String> {
        if let Value::NodeSlot(crate::domain::node_slots::NodeSlot::Content(nodes)) = value {
            let mut next = Vec::new();
            for (index, child) in std::mem::take(nodes).into_iter().enumerate() {
                next.extend(visit(
                    child,
                    scope,
                    &format!("{path}/{index}"),
                    count,
                    depth + 1,
                )?);
            }
            *nodes = next;
        }
        Ok(())
    }
    fn visit(
        mut node: Node,
        scope: &str,
        path: &str,
        count: &mut usize,
        depth: usize,
    ) -> Result<Vec<Node>, String> {
        *count += 1;
        if *count > 16384 || depth > 128 {
            return Err("repeat projection exceeds node/depth budget".into());
        }
        let resolve = |value: &mut Value| -> Result<(), String> {
            if let Value::Predicate(expression) = value
                && !expression.contains_local()
                && !expression.contains_state()
            {
                *value = Value::Bool(
                    expression
                        .evaluate(&std::collections::BTreeMap::new())
                        .map_err(|error| error.message)?,
                );
            }
            if let Value::Member(member) = value {
                *value = member
                    .bound()?
                    .ok_or("repeat record member was not bound")?;
            }
            Ok(())
        };
        match &mut node.kind {
            NodeKind::Text(Value::Repeat(repeat)) => resolve(&mut repeat.source)?,
            NodeKind::Text(value) => resolve(value)?,
            NodeKind::Element { attributes, .. } => {
                for attribute in attributes {
                    resolve(&mut attribute.value)?;
                }
            }
        }
        if let NodeKind::Text(Value::Repeat(repeat)) = &node.kind {
            if matches!(repeat.source, Value::State(_)) {
                // State-driven lists expand per state projection, with the current items.
                return Ok(vec![node]);
            }
            let Value::Array(array) = &repeat.source else {
                return Err("repeat source was not bound".into());
            };
            array.validate()?;
            let mut keys = BTreeSet::new();
            let mut key_bytes = 0usize;
            let mut output = Vec::with_capacity(array.items.len());
            for value in &array.items {
                let key_value = if repeat.key_path.is_empty() {
                    value.clone()
                } else {
                    crate::domain::member_value::MemberValue {
                        root: Box::new(value.clone()),
                        path: repeat.key_path.clone(),
                    }
                    .bound()?
                    .ok_or("repeat key record was not bound")?
                };
                let Value::String(key) = key_value else {
                    return Err("repeat item key requires string elements".into());
                };
                key_bytes = key_bytes
                    .checked_add(key.len())
                    .ok_or("repeat key bytes overflow")?;
                if key.len() > 4096 || key_bytes > 1024 * 1024 || !keys.insert(key.clone()) {
                    return Err("repeat keys must be unique and within byte budgets".into());
                }
                let mut template = repeat.template.clone();
                crate::domain::repeat::substitute_local(&mut template, &repeat.item, value);
                let identity = ContentHash::of(
                    &serde_json::to_vec(&(scope, path, key)).map_err(|error| error.to_string())?,
                );
                let NodeKind::Element { attributes, .. } = &mut template.kind else {
                    return Err("repeat template requires an element".into());
                };
                if attributes
                    .iter()
                    .any(|attribute| attribute.name == IDENTITY_ATTRIBUTE)
                {
                    return Err("repeat identity attribute is reserved".into());
                }
                attributes.push(Attribute {
                    name: IDENTITY_ATTRIBUTE.into(),
                    value: Value::String(identity.as_str().into()),
                });
                output.extend(visit(
                    template,
                    scope,
                    &format!("{path}/key:{}", identity.as_str()),
                    count,
                    depth + 1,
                )?);
            }
            return Ok(output);
        }
        let mut node = node;
        if let NodeKind::Text(value) = &mut node.kind {
            slot(value, scope, &format!("{path}/slot"), count, depth)?;
        }
        if let NodeKind::Element {
            children,
            attributes,
            ..
        } = &mut node.kind
        {
            for attribute in attributes {
                slot(
                    &mut attribute.value,
                    scope,
                    &format!("{path}/slot:{}", attribute.name),
                    count,
                    depth,
                )?;
            }
            let mut next = Vec::new();
            for (index, child) in std::mem::take(children).into_iter().enumerate() {
                next.extend(visit(
                    child,
                    scope,
                    &format!("{path}/{index}"),
                    count,
                    depth + 1,
                )?);
            }
            *children = next;
        }
        Ok(vec![node])
    }
    let mut roots = visit(
        component.root.clone(),
        scope.unwrap_or(&component.name),
        "0",
        &mut 0,
        0,
    )?;
    if roots.len() != 1 {
        return Err("component requires one root".into());
    }
    component.root = roots.remove(0);
    Ok(())
}

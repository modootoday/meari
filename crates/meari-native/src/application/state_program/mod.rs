use meari::domain::{Component, Node, NodeKind, Value};

pub(crate) fn validate(component: &Component) -> Result<(), String> {
    validate_callbacks(component, false)
}

#[cfg(test)]
mod boolean_admission_tests;

pub(crate) fn validate_callbacks(component: &Component, callbacks: bool) -> Result<(), String> {
    fn visit(node: &Node, callbacks: bool) -> Result<(), String> {
        if let NodeKind::Text(Value::NodeSlot(meari::domain::node_slots::NodeSlot::Content(
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
            super::markup::composition::validate_handlers(node)?;
            for attribute in attributes {
                let callback_source = match &attribute.value {
                    Value::Callback(handler) => Some(handler.source),
                    Value::BoundCallback(handler) => Some(handler.source),
                    _ => None,
                };
                if callback_source
                    == Some(meari::domain::host_callbacks::CallbackSource::BooleanValue)
                {
                    if !callbacks {
                        return Err("native Boolean callbacks require an attached scoped document dispatcher".into());
                    }
                    super::boolean_profile::validate(tag, &attribute.name, attributes)?;
                }
                if let Value::NodeSlot(meari::domain::node_slots::NodeSlot::Content(nodes)) =
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
                        "native host callbacks require an attached scoped document dispatcher"
                            .into(),
                    );
                }
                if let Value::Input(binding) = &attribute.value {
                    let input_type = attributes
                        .iter()
                        .find(|attribute| attribute.name == "type")
                        .and_then(|attribute| match &attribute.value {
                            Value::String(value) => Some(value.as_str()),
                            _ => None,
                        })
                        .unwrap_or("text");
                    let supported = ["Input", "input"].contains(&tag.as_str())
                        && match binding.kind {
                            meari::domain::input_event::InputKind::Text => {
                                ["text", "search", "email", "url", "tel", "password", "range"]
                                    .contains(&input_type)
                            }
                            meari::domain::input_event::InputKind::Checked => {
                                input_type == "checkbox"
                            }
                        };
                    if !supported {
                        return Err(
                            "native source input kind requires a dedicated control adapter".into(),
                        );
                    }
                    let property = match binding.kind {
                        meari::domain::input_event::InputKind::Text => "value",
                        meari::domain::input_event::InputKind::Checked => "checked",
                    };
                    if !attributes
                        .iter()
                        .any(|attribute| attribute.name == property)
                    {
                        return Err("native source input requires its controlled property".into());
                    }
                }
                if matches!(attribute.value, Value::State(_))
                    && ["open", "value", "checked", "className"].contains(&attribute.name.as_str())
                    && !(["value", "checked"].contains(&attribute.name.as_str())
                        && attributes
                            .iter()
                            .any(|attribute| matches!(attribute.value, Value::Input(_))))
                {
                    return Err(format!(
                        "dynamic source {} requires controlled callbacks/style dependencies",
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

pub(crate) fn retained_shape(previous: &Node, next: &Node) -> Result<(), String> {
    match (&previous.kind, &next.kind) {
        (NodeKind::Text(_), NodeKind::Text(_)) => Ok(()),
        (
            NodeKind::Element {
                tag,
                attributes,
                children,
            },
            NodeKind::Element {
                tag: next_tag,
                attributes: next_attributes,
                children: next_children,
            },
        ) if tag == next_tag => {
            for identity in ["id", "type", "role"] {
                let before = attributes
                    .iter()
                    .find(|attribute| attribute.name == identity)
                    .map(|attribute| &attribute.value);
                let after = next_attributes
                    .iter()
                    .find(|attribute| attribute.name == identity)
                    .map(|attribute| &attribute.value);
                if before != after {
                    return Err(
                        "source control identity changed; document remount is required".into(),
                    );
                }
            }
            if crate::domain::widgets::family(tag).is_some()
                || [
                    "TabsContent",
                    "TabsTrigger",
                    "AccordionItem",
                    "SelectItem",
                    "DropdownMenuItem",
                ]
                .contains(&tag.as_str())
            {
                for controlled in ["value", "open"] {
                    let before = attributes
                        .iter()
                        .find(|attribute| attribute.name == controlled)
                        .map(|attribute| &attribute.value);
                    let after = next_attributes
                        .iter()
                        .find(|attribute| attribute.name == controlled)
                        .map(|attribute| &attribute.value);
                    if controlled == "open"
                        && ["Dialog", "AlertDialog"].contains(&tag.as_str())
                        && matches!(before, Some(Value::Bool(_)))
                        && matches!(after, Some(Value::Bool(_)))
                    {
                        // Accepted controlled props are authoritative; changing control mode still remounts.
                        continue;
                    }
                    if controlled == "value"
                        && tag == "RadioGroup"
                        && matches!(before, Some(Value::String(_)))
                        && matches!(after, Some(Value::String(_)))
                    {
                        continue;
                    }
                    if before != after {
                        return Err(
                            "controlled widget source changed; controlled migration is required"
                                .into(),
                        );
                    }
                }
            }
            fn key(node: &Node) -> Option<&str> {
                if let NodeKind::Element { attributes, .. } = &node.kind {
                    return attributes
                        .iter()
                        .find_map(|attribute| match &attribute.value {
                            Value::String(value)
                                if attribute.name
                                    == meari::application::repeat::IDENTITY_ATTRIBUTE =>
                            {
                                Some(value.as_str())
                            }
                            _ => None,
                        });
                }
                None
            }
            let before = children.iter().map(key).collect::<Vec<_>>();
            let after = next_children.iter().map(key).collect::<Vec<_>>();
            let matches = meari::application::source_modules::child_placement::plan(
                &before, &after, next.span,
            )
            .map_err(|error| error.message)?;
            for (child, previous) in next_children.iter().zip(matches) {
                if let Some(previous) = previous {
                    retained_shape(&children[previous], child)?;
                }
            }
            Ok(())
        }
        _ => Err("source node shape changed; document remount is required".into()),
    }
}

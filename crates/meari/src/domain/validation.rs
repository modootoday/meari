use crate::domain::*;
use std::collections::HashSet;

pub(crate) fn identifier(name: &str) -> bool {
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
    crate::domain::node_slots::validate_tree(&component.root)
        .map_err(|message| Diagnostic::unsupported(component.root.span, message))?;
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
    let mut state_names = HashSet::new();
    for binding in &component.states {
        for name in [&binding.name, &binding.setter] {
            if !identifier(name)
                || ["props", "useState", "useId", "Props"].contains(&name.as_str())
                || name == &component.name
                || !state_names.insert(name)
            {
                return Err(Diagnostic::unsupported(
                    binding.span,
                    "invalid or duplicate state binding/setter",
                ));
            }
        }
        if let component_state::StateValue::Structured(expression) = &binding.initial {
            expression.validate_literal()?;
        } else {
            crate::domain::state_program::render_value(&binding.initial)
                .map_err(|message| Diagnostic::unsupported(binding.span, message))?;
        }
    }
    let ids = crate::domain::hooks::ids(&component.root);
    if !component.hook_order.is_empty() && (component.states.is_empty() || ids.is_empty()) {
        return Err(err(
            "explicit hook order is reserved for mixed ID/state layouts",
        ));
    }
    if !component.hook_order.is_empty() || (!component.states.is_empty() && !ids.is_empty()) {
        let mut ordered_ids = std::collections::BTreeSet::new();
        let mut ordered_states = Vec::new();
        for hook in &component.hook_order {
            match hook {
                crate::domain::hooks::HookSlot::Id(name) => {
                    if !ordered_ids.insert(name.clone()) {
                        return Err(err("duplicate ID hook slot"));
                    }
                }
                crate::domain::hooks::HookSlot::State(name) => ordered_states.push(name),
            }
        }
        if ordered_ids != ids
            || ordered_states
                != component
                    .states
                    .iter()
                    .map(|state| &state.name)
                    .collect::<Vec<_>>()
        {
            return Err(err(
                "hook order must cover each ID and state declaration exactly once",
            ));
        }
    }
    for name in crate::domain::hooks::ids(&component.root) {
        if !identifier(&name)
            || ["props", "useId", "Props"].contains(&name.as_str())
            || name == component.name
            || state_names.contains(&name)
            || crate::domain::design::components::module(&name).is_some()
        {
            return Err(err("invalid or shadowing useId binding"));
        }
    }
    for prop in &component.props {
        prop.kind
            .validate()
            .map_err(|message| Diagnostic::unsupported(component.root.span, message))?;
        if !identifier(&prop.name) || !names.insert(&prop.name) {
            return Err(err("invalid or duplicate prop"));
        }
    }
    fn repeat_state_scope(
        root: &Node,
        component: &Component,
        shadowed: &str,
    ) -> Result<(), Diagnostic> {
        fn value<'a>(
            value: &'a Value,
            span: Span,
            component: &Component,
            shadowed: &str,
            pending: &mut Vec<&'a Node>,
        ) -> Result<(), Diagnostic> {
            let fail = |span| {
                Diagnostic::unsupported(span, "state read is shadowed by a lexical repeat item")
            };
            match value {
                Value::State(name) if name == shadowed => return Err(fail(span)),
                Value::Predicate(expression) => {
                    // Repeat.validate supplies each lexical item's declared schema before
                    // full validation. The scope walk still rejects shadowed state reads.
                    if !expression.contains_local() {
                        expression.validate_with_states(&component.props, &component.states)?;
                    }
                    if let Some(span) = expression.state_reference_span(shadowed) {
                        return Err(fail(span));
                    }
                }
                Value::NodeSlot(super::node_slots::NodeSlot::Content(nodes)) => {
                    pending.extend(nodes)
                }
                Value::Repeat(repeat) => pending.push(&repeat.template),
                _ => {}
            }
            Ok(())
        }
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            match &node.kind {
                NodeKind::Text(v) => value(v, node.span, component, shadowed, &mut pending)?,
                NodeKind::Element {
                    attributes,
                    children,
                    ..
                } => {
                    for attribute in attributes {
                        value(
                            &attribute.value,
                            node.span,
                            component,
                            shadowed,
                            &mut pending,
                        )?;
                    }
                    pending.extend(children);
                }
            }
        }
        Ok(())
    }

    fn value_type(
        value: &Value,
        component: &Component,
        span: Span,
    ) -> Result<PropType, Diagnostic> {
        match value {
            Value::NodeSlot(slot) => {
                if let crate::domain::node_slots::NodeSlot::Content(nodes) = slot {
                    for child in nodes {
                        node(child, component)?;
                    }
                }
                Ok(PropType::Node)
            }
            Value::Local(_) | Value::Repeat(_) => Err(Diagnostic::unsupported(
                span,
                "lexical/repeat expression requires its repeat scope",
            )),
            Value::String(_) => Ok(PropType::String),
            Value::Id(_) => Ok(PropType::String),
            Value::Bool(_) => Ok(PropType::Bool),
            Value::Array(array) => {
                array
                    .validate()
                    .map_err(|message| Diagnostic::unsupported(span, message))?;
                Ok(PropType::Array(Box::new(array.element.clone())))
            }
            Value::Record(record) => {
                record
                    .validate()
                    .map_err(|message| Diagnostic::unsupported(span, message))?;
                Ok(record.kind())
            }
            Value::Member(member) => member
                .kind(&component.props)
                .map_err(|message| Diagnostic::unsupported(span, message)),
            Value::Predicate(expression) => {
                if matches!(
                    expression.kind,
                    super::bool_expression::BoolExpressionKind::State(_)
                ) {
                    return Err(Diagnostic::unsupported(
                        span,
                        "direct state read requires canonical Value::State",
                    ));
                }
                expression.validate_with_states(&component.props, &component.states)?;
                Ok(PropType::Bool)
            }
            Value::NotProp(name) => {
                if component
                    .props
                    .iter()
                    .any(|prop| prop.name == *name && prop.kind == PropType::Bool)
                {
                    Ok(PropType::Bool)
                } else {
                    Err(Diagnostic::unsupported(
                        span,
                        "boolean negation requires a declared boolean prop",
                    ))
                }
            }
            Value::State(name) => {
                let binding = component
                    .states
                    .iter()
                    .find(|binding| binding.name == *name)
                    .ok_or_else(|| Diagnostic::unsupported(span, "unknown state binding"))?;
                match &binding.initial {
                    crate::domain::component_state::StateValue::Bool(_) => Ok(PropType::Bool),
                    _ => Ok(PropType::String),
                }
            }
            Value::CallbackReference(reference) => {
                reference
                    .validate()
                    .map_err(|message| Diagnostic::unsupported(span, message))?;
                Ok(PropType::Callback(reference.signature))
            }
            Value::Event(_) | Value::Input(_) | Value::Callback(_) | Value::BoundCallback(_) => {
                Err(Diagnostic::unsupported(
                    span,
                    "state event is only valid on an event attribute",
                ))
            }
            Value::Prop(name) => component
                .props
                .iter()
                .find(|p| p.name == *name)
                .map(|p| p.kind.clone())
                .ok_or_else(|| Diagnostic::unsupported(span, format!("unknown prop {name}"))),
        }
    }
    fn node(node: &Node, component: &Component) -> Result<(), Diagnostic> {
        let err = |message| Diagnostic::unsupported(node.span, message);
        match &node.kind {
            NodeKind::Text(value) => {
                if let Value::Repeat(repeat) = value {
                    repeat_state_scope(&repeat.template, component, &repeat.item)?;
                    return repeat.validate(component);
                }
                if !matches!(
                    value_type(value, component, node.span)?,
                    PropType::String | PropType::Node
                ) {
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
                    if name.starts_with("data-meari-radio-") {
                        return Err(err("radio renderer metadata is reserved"));
                    }
                    let state_event = (name == "onClick"
                        && matches!(attribute.value, Value::Event(_)))
                        || (name == "onChange" && matches!(attribute.value, Value::Input(_)));
                    let callback_event = match &attribute.value {
                        Value::Callback(handler) => {
                            let signature = component
                                .props
                                .iter()
                                .find(|prop| prop.name == handler.prop)
                                .map(|prop| prop.kind.clone());
                            if signature != Some(PropType::Callback(handler.signature())) {
                                return Err(Diagnostic::unsupported(
                                    handler.span,
                                    "callback signature differs from declared prop",
                                ));
                            }
                            match (handler.source, &handler.capture_prop) {
                                (
                                    crate::domain::host_callbacks::CallbackSource::CapturedText,
                                    Some(name),
                                ) if component.props.iter().any(|prop| {
                                    prop.name == *name && prop.kind == PropType::String
                                }) => {}
                                (
                                    crate::domain::host_callbacks::CallbackSource::CapturedText,
                                    _,
                                ) => {
                                    return Err(Diagnostic::unsupported(
                                        handler.span,
                                        "callback capture requires a declared string prop",
                                    ));
                                }
                                (_, Some(_)) => {
                                    return Err(Diagnostic::unsupported(
                                        handler.span,
                                        "callback source does not accept a captured prop",
                                    ));
                                }
                                (_, None) => {}
                            }
                            Some(handler.source)
                        }
                        Value::BoundCallback(handler) => {
                            handler.validate().map_err(|message| {
                                Diagnostic::unsupported(handler.span, message)
                            })?;
                            Some(handler.source)
                        }
                        _ => None,
                    };
                    let callback_event = callback_event.is_some_and(|source| match source {
                        crate::domain::host_callbacks::CallbackSource::Action
                        | crate::domain::host_callbacks::CallbackSource::CapturedText => {
                            name == "onClick" && ["button", "Button"].contains(&tag.as_str())
                        }
                        crate::domain::host_callbacks::CallbackSource::CurrentTargetText => {
                            (name == "onChange"
                                && ["input", "Input", "textarea"].contains(&tag.as_str()))
                                || (name == "onValueCommit"
                                    && ["input", "Input"].contains(&tag.as_str())
                                    && attributes.iter().any(|attribute| {
                                        attribute.name == "type"
                                            && attribute.value == Value::String("range".into())
                                    }))
                        }
                        crate::domain::host_callbacks::CallbackSource::BooleanValue => {
                            (name == "onOpenChange"
                                && ["Dialog", "AlertDialog"].contains(&tag.as_str()))
                                || (name == "onCheckedChange"
                                    && ["Switch", "Checkbox"].contains(&tag.as_str()))
                        }
                        crate::domain::host_callbacks::CallbackSource::SelectionValue => {
                            name == "onValueChange" && tag == "RadioGroup"
                        }
                    });
                    if matches!(
                        attribute.value,
                        Value::Callback(_) | Value::BoundCallback(_)
                    ) && !callback_event
                    {
                        return Err(err(
                            "callback event requires a matching button/input handler",
                        ));
                    }
                    let callback_forward = tag.starts_with(|ch: char| ch.is_ascii_uppercase())
                        && !["Button", "Input", "RadioGroup", "RadioGroupItem"]
                            .contains(&tag.as_str())
                        && matches!(
                            value_type(&attribute.value, component, node.span),
                            Ok(PropType::Callback(_))
                        );
                    if (name.starts_with("on")
                        && !state_event
                        && !callback_event
                        && !callback_forward)
                        || ["key", "ref", "dangerouslySetInnerHTML"].contains(&name.as_str())
                        || (name == "children"
                            && !(tag.starts_with(|ch: char| ch.is_ascii_uppercase())
                                && value_type(&attribute.value, component, node.span)?
                                    == PropType::Node))
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
                    if callback_event {
                        continue;
                    } else if let Value::Input(binding) = &attribute.value {
                        if name != "onChange"
                            || !["Input", "input", "textarea", "select"].contains(&tag.as_str())
                        {
                            return Err(err(
                                "typed source input event requires onChange on an input control",
                            ));
                        }
                        if binding.kind == crate::domain::input_event::InputKind::Checked
                            && (!["Input", "input"].contains(&tag.as_str())
                                || !attributes.iter().any(|attribute| {
                                    attribute.name == "type"
                                        && attribute.value == Value::String("checkbox".into())
                                }))
                        {
                            return Err(err(
                                "checked input payload requires an explicit checkbox input",
                            ));
                        }
                        binding
                            .validate(&component.states)
                            .map_err(|message| Diagnostic::unsupported(node.span, message))?;
                    } else if let Value::Event(event) = &attribute.value {
                        if !["Button", "button"].contains(&tag.as_str()) || !state_event {
                            return Err(err(
                                "source state onClick currently requires Button/button",
                            ));
                        }
                        event
                            .validate(&component.states)
                            .map_err(|message| Diagnostic::unsupported(node.span, message))?;
                    } else {
                        let kind = value_type(&attribute.value, component, node.span)?;
                        if matches!(
                            kind,
                            PropType::Array(_) | PropType::Record(_) | PropType::Node
                        ) && !tag.chars().next().is_some_and(char::is_uppercase)
                        {
                            return Err(err(
                                "array values require a typed module prop or keyed repeat",
                            ));
                        }
                        if matches!(kind, PropType::Callback(_)) && !callback_forward {
                            return Err(err(
                                "callback references require an explicit event or module prop",
                            ));
                        }
                    }
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

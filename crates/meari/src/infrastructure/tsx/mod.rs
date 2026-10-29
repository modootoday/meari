use crate::domain::{self, *};
mod conditional;
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};
mod bool_expression;
mod callbacks;
mod prop_type;
mod repeat;
mod source_modules;
mod state;

fn location(n: &impl GetSpan) -> domain::Span {
    let s = n.span();
    domain::Span {
        start: s.start as usize,
        end: s.end as usize,
    }
}
fn err(n: &impl GetSpan, message: &str) -> Diagnostic {
    Diagnostic::unsupported(location(n), message)
}

fn value(expr: &Expression<'_>) -> Result<Value, Diagnostic> {
    match expr {
        Expression::NullLiteral(_) => Ok(Value::NodeSlot(node_slots::NodeSlot::Empty)),
        Expression::JSXElement(node) => Ok(Value::NodeSlot(node_slots::NodeSlot::Content(vec![
            element(node)?,
        ]))),
        Expression::JSXFragment(fragment) => Ok(Value::NodeSlot(node_slots::NodeSlot::Content(
            jsx_children(&fragment.children)?,
        ))),
        Expression::StringLiteral(s) => Ok(Value::String(s.value.to_string())),
        Expression::BooleanLiteral(b) => Ok(Value::Bool(b.value)),
        Expression::LogicalExpression(_) | Expression::BinaryExpression(_) => {
            bool_expression::parse(expr)
                .map(Box::new)
                .map(Value::Predicate)
        }
        Expression::ParenthesizedExpression(value_) => value(&value_.expression),
        Expression::UnaryExpression(unary)
            if unary.operator == oxc_syntax::operator::UnaryOperator::LogicalNot =>
        {
            match value(&unary.argument)? {
                Value::Prop(prop) => Ok(Value::NotProp(prop)),
                _ => bool_expression::parse(expr)
                    .map(Box::new)
                    .map(Value::Predicate),
            }
        }
        Expression::ArrowFunctionExpression(arrow) => {
            if matches!(arrow.body.as_expression(), Some(Expression::CallExpression(call)) if matches!(&call.callee, Expression::StaticMemberExpression(member) if matches!(&member.object, Expression::Identifier(name) if name.name == "props")))
            {
                crate::infrastructure::source_callbacks::lower(expr).map(Value::Callback)
            } else {
                state::event(arrow)
            }
        }
        Expression::Identifier(i) => Ok(Value::Id(i.name.to_string())),
        Expression::StaticMemberExpression(m)
            if !m.optional
                && matches!(&m.object, Expression::Identifier(i) if i.name == "props") =>
        {
            Ok(Value::Prop(m.property.name.to_string()))
        }
        Expression::StaticMemberExpression(member) if !member.optional => {
            let (root, mut path) = match value(&member.object)? {
                Value::Id(name) => (Value::Local(name), Vec::new()),
                Value::Prop(name) => (Value::Prop(name), Vec::new()),
                Value::Member(member) => (*member.root, member.path),
                _ => {
                    return Err(err(
                        expr,
                        "member requires a declared record prop or repeat item",
                    ));
                }
            };
            if path.len() >= 32 {
                return Err(err(expr, "record member exceeds depth budget"));
            }
            path.push(member.property.name.to_string());
            Ok(Value::Member(member_value::MemberValue {
                root: Box::new(root),
                path,
            }))
        }
        _ => Err(err(
            expr,
            "only string/bool literals and props.name supported; hooks/effects/expressions unsupported",
        )),
    }
}

fn jsx_value(expr: &JSXExpression<'_>) -> Result<Value, Diagnostic> {
    expr.as_expression()
        .ok_or_else(|| err(expr, "empty JSX expression unsupported"))
        .and_then(value)
}

fn jsx_text(text: &str) -> String {
    let allocator = Allocator::default();
    let mut decoded = None;
    oxc_syntax::xml_entities::decode_entities(text, &mut decoded, text.len(), &allocator);
    decoded.map_or_else(|| text.to_owned(), |value| value.as_str().to_owned())
}

fn literal_text(text: &str) -> String {
    use oxc_syntax::{identifier::is_white_space_single_line, line_terminator::is_line_terminator};
    let lines: Vec<_> = text.split(is_line_terminator).collect();
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = if index == 0 {
                *line
            } else {
                line.trim_start_matches(is_white_space_single_line)
            };
            let line = if index + 1 == lines.len() {
                line
            } else {
                line.trim_end_matches(is_white_space_single_line)
            };
            (!line.is_empty()).then(|| jsx_text(line))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn element(e: &JSXElement<'_>) -> Result<Node, Diagnostic> {
    let tag = match &e.opening_element.name {
        JSXElementName::Identifier(i) => i.name.to_string(),
        JSXElementName::IdentifierReference(i) => i.name.to_string(),
        _ => return Err(err(e, "namespaced/member JSX names unsupported")),
    };
    if e.opening_element.type_arguments.is_some() {
        return Err(err(e, "generic JSX components unsupported"));
    }
    let mut attributes = Vec::new();
    for attr in &e.opening_element.attributes {
        let JSXAttributeItem::Attribute(a) = attr else {
            return Err(err(attr, "spread attributes unsupported"));
        };
        let JSXAttributeName::Identifier(name) = &a.name else {
            return Err(err(a.as_ref(), "namespaced attributes unsupported"));
        };
        let mut value = match &a.value {
            None => Value::Bool(true),
            Some(JSXAttributeValue::StringLiteral(s)) => Value::String(jsx_text(s.value.as_str())),
            Some(JSXAttributeValue::ExpressionContainer(c)) => jsx_value(&c.expression)?,
            _ => {
                return Err(err(
                    a.as_ref(),
                    "only primitive JSX attribute values supported",
                ));
            }
        };
        use crate::domain::host_callbacks::CallbackSource;
        let callback_source = match (name.name.as_str(), tag.as_str()) {
            ("onClick", "button" | "Button") => Some(CallbackSource::Action),
            ("onOpenChange", "Dialog" | "AlertDialog")
            | ("onCheckedChange", "Switch" | "Checkbox") => Some(CallbackSource::BooleanValue),
            ("onValueChange", "RadioGroup") => Some(CallbackSource::SelectionValue),
            ("onValueCommit", "input" | "Input") => Some(CallbackSource::CurrentTargetText),
            _ => None,
        };
        if let (Some(source), Value::Prop(prop)) = (callback_source, &value) {
            value = Value::Callback(crate::domain::host_callbacks::CallbackHandler {
                prop: prop.clone(),
                source,
                capture_prop: None,
                span: location(a.as_ref()),
            });
        }
        if callback_source == Some(CallbackSource::SelectionValue)
            && let Value::Callback(handler) = &mut value
            && handler.source == CallbackSource::BooleanValue
        {
            handler.source = CallbackSource::SelectionValue;
        }
        if name.name == "onValueCommit"
            && callback_source == Some(CallbackSource::CurrentTargetText)
            && let Value::Callback(handler) = &mut value
            && handler.source == CallbackSource::BooleanValue
        {
            handler.source = CallbackSource::CurrentTargetText;
        }
        attributes.push(Attribute {
            name: name.name.to_string(),
            value,
        });
    }
    let children = jsx_children(&e.children)?;
    Ok(Node {
        kind: NodeKind::Element {
            tag,
            attributes,
            children,
        },
        span: location(e),
    })
}
fn jsx_children(input: &[JSXChild<'_>]) -> Result<Vec<Node>, Diagnostic> {
    let mut children = Vec::new();
    for child in input {
        match child {
            JSXChild::Element(e) => children.push(element(e)?),
            JSXChild::Fragment(fragment) => children.push(Node {
                kind: NodeKind::Text(Value::NodeSlot(node_slots::NodeSlot::Content(
                    jsx_children(&fragment.children)?,
                ))),
                span: location(fragment.as_ref()),
            }),
            JSXChild::ExpressionContainer(c)
                if matches!(c.expression, JSXExpression::EmptyExpression(_)) => {}
            JSXChild::ExpressionContainer(c) => {
                if let Some(expression @ Expression::CallExpression(_)) =
                    c.expression.as_expression()
                {
                    children.extend(repeat::nodes(expression)?);
                } else if let Some(node) = conditional::node(c.expression.as_expression())? {
                    children.push(node);
                } else {
                    children.push(Node {
                        kind: NodeKind::Text(jsx_value(&c.expression)?),
                        span: location(c.as_ref()),
                    });
                }
            }
            JSXChild::Text(t) => {
                let text = literal_text(t.value.as_str());
                if !text.is_empty() {
                    children.push(Node {
                        kind: NodeKind::Text(Value::String(text)),
                        span: location(t.as_ref()),
                    });
                }
            }
            _ => return Err(err(child, "fragments/spread children unsupported")),
        }
    }
    Ok(children)
}

pub fn parse(source: &str) -> Result<Component, Diagnostic> {
    parse_source(source, false)
}

pub(crate) fn parse_module(
    source: &str,
) -> Result<crate::application::source_modules::ParsedSourceModule, Diagnostic> {
    let imports = source_modules::local_imports(source)?;
    let component = parse_source(source, true)?;
    crate::domain::validation::validate(&component)?;
    Ok(crate::application::source_modules::ParsedSourceModule { component, imports })
}

fn parse_source(source: &str, local_modules: bool) -> Result<Component, Diagnostic> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::tsx()).parse();
    if let Some(error) = parsed.diagnostics.first() {
        return Err(Diagnostic {
            code: "syntax".into(),
            message: error.to_string(),
            span: error
                .labels
                .first()
                .map_or_else(domain::Span::default, |label| domain::Span {
                    start: label.offset() as usize,
                    end: (label.offset() + label.len()) as usize,
                }),
        });
    }
    if parsed.program.hashbang.is_some()
        || parsed
            .program
            .directives
            .iter()
            .any(|d| d.directive != "use client")
        || parsed.program.directives.len() > 1
    {
        return Err(err(
            &parsed.program,
            "only one use client module directive supported; hashbang/server/other directives unsupported",
        ));
    }
    let mut props = None;
    let mut function = None;
    let mut imported = std::collections::BTreeSet::new();
    let mut use_id = false;
    let mut use_state = false;
    for statement in &parsed.program.body {
        match statement {
            Statement::ImportDeclaration(import) => {
                if import.source.value == "react" && import.import_kind == ImportOrExportKind::Type && import.phase.is_none() && import.with_clause.is_none() && import.specifiers.as_ref().is_some_and(|specifiers| specifiers.len() == 1 && matches!(&specifiers[0],ImportDeclarationSpecifier::ImportSpecifier(specifier) if matches!(&specifier.imported, ModuleExportName::IdentifierName(name) if name.name == "ReactNode") && specifier.local.name == "ReactNode")) { continue; }
                if import.phase.is_some()
                    || import.with_clause.is_some()
                    || import.import_kind != ImportOrExportKind::Value
                {
                    return Err(err(
                        import.as_ref(),
                        "plain source-owned component imports required",
                    ));
                }
                let Some(specifiers) = &import.specifiers else {
                    return Err(err(import.as_ref(), "side-effect imports unsupported"));
                };
                if specifiers.is_empty() {
                    return Err(err(import.as_ref(), "empty imports unsupported"));
                }
                for specifier in specifiers {
                    let ImportDeclarationSpecifier::ImportSpecifier(specifier) = specifier else {
                        return Err(err(
                            import.as_ref(),
                            "only named component imports supported",
                        ));
                    };
                    let ModuleExportName::IdentifierName(name) = &specifier.imported else {
                        return Err(err(import.as_ref(), "component identifier import required"));
                    };
                    if import.source.value == "react"
                        && name.name == "useState"
                        && specifier.local.name == "useState"
                        && specifier.import_kind == ImportOrExportKind::Value
                        && !use_state
                    {
                        use_state = true;
                        continue;
                    }
                    if import.source.value == "react"
                        && name.name == "useId"
                        && specifier.local.name == "useId"
                        && specifier.import_kind == ImportOrExportKind::Value
                        && !use_id
                    {
                        use_id = true;
                        continue;
                    }
                    let module = crate::domain::design::components::module(name.name.as_str());
                    if local_modules
                        && (import.source.value.starts_with("./")
                            || import.source.value.starts_with("../"))
                    {
                        if !imported.insert(specifier.local.name.to_string()) {
                            return Err(err(import.as_ref(), "duplicate local component binding"));
                        }
                        continue;
                    }
                    if specifier.import_kind != ImportOrExportKind::Value
                        || name.name != specifier.local.name
                        || module.is_none_or(|module| {
                            import.source.value != format!("@/components/ui/{module}")
                        })
                        || !imported.insert(name.name.to_string())
                    {
                        return Err(err(
                            import.as_ref(),
                            "unregistered, aliased, type-only or duplicate component import",
                        ));
                    }
                }
            }
            Statement::TSTypeAliasDeclaration(alias)
                if props.is_none() && alias.id.name == "Props" =>
            {
                if alias.declare || alias.type_parameters.is_some() {
                    return Err(err(alias.as_ref(), "plain Props alias required"));
                }
                let TSType::TSTypeLiteral(literal) = &alias.type_annotation else {
                    return Err(err(alias.as_ref(), "Props must be an object literal type"));
                };
                let mut fields = Vec::new();
                for member in &literal.members {
                    let TSSignature::TSPropertySignature(p) = member else {
                        return Err(err(member, "only required primitive props supported"));
                    };
                    if p.optional || p.computed || p.readonly {
                        return Err(err(
                            p.as_ref(),
                            "optional/computed/readonly props unsupported",
                        ));
                    }
                    let PropertyKey::StaticIdentifier(key) = &p.key else {
                        return Err(err(p.as_ref(), "simple prop identifier required"));
                    };
                    let annotation = p
                        .type_annotation
                        .as_ref()
                        .ok_or_else(|| err(p.as_ref(), "prop type required"))?;
                    let kind = prop_type::parse(&annotation.type_annotation, 0)?;
                    fields.push(Prop {
                        name: key.name.to_string(),
                        kind,
                    });
                }
                props = Some(fields);
            }
            Statement::ExportDeclaration(export) if function.is_none() => {
                let Declaration::FunctionDeclaration(f) = &export.declaration else {
                    return Err(err(export.as_ref(), "export one named function component"));
                };
                function = Some(f.as_ref());
            }
            _ => {
                return Err(err(
                    statement,
                    "only type Props and one export function supported; imports/module effects unsupported",
                ));
            }
        }
    }
    let f = function.ok_or_else(|| {
        Diagnostic::unsupported(
            domain::Span::default(),
            "export function component required",
        )
    })?;
    if f.r#async
        || f.generator
        || f.declare
        || f.type_parameters.is_some()
        || f.this_param.is_some()
        || f.return_type.is_some()
        || f.params.rest.is_some()
        || f.params.items.len() > 1
    {
        return Err(err(f, "plain synchronous function(props: Props) required"));
    }
    if let Some(p) = f.params.items.first() {
        if !p.decorators.is_empty()
            || p.optional
            || p.initializer.is_some()
            || p.accessibility.is_some()
            || p.readonly
            || p.r#override
            || !matches!(&p.pattern, BindingPattern::BindingIdentifier(i) if i.name == "props")
        {
            return Err(err(p, "single props identifier required"));
        }
        let valid_type = p.type_annotation.as_ref().is_some_and(|a| matches!(&a.type_annotation, TSType::TSTypeReference(r) if r.type_arguments.is_none() && matches!(&r.type_name, TSTypeName::IdentifierReference(i) if i.name == "Props")));
        if !valid_type {
            return Err(err(p, "parameter must have Props type"));
        }
    } else {
        if props.as_ref().is_some_and(|fields| !fields.is_empty()) {
            return Err(err(f, "parameterless components require no Props fields"));
        }
        props = Some(Vec::new());
    }
    let body = f
        .body
        .as_ref()
        .ok_or_else(|| err(f, "function body required"))?;
    if !body.directives.is_empty() {
        return Err(err(body.as_ref(), "body directives unsupported"));
    }
    let Some((Statement::ReturnStatement(ret), declarations)) =
        body.statements.as_slice().split_last()
    else {
        return Err(err(
            body.as_ref(),
            "one return only; state/hooks/effects unsupported",
        ));
    };
    let mut ids = std::collections::BTreeSet::new();
    let mut states = Vec::new();
    let mut hook_order = Vec::new();
    for statement in declarations {
        if use_state
            && matches!(statement, Statement::VariableDeclaration(declaration) if declaration.declarations.iter().any(|binding| matches!(binding.id, BindingPattern::ArrayPattern(_))))
        {
            let binding = state::declaration(statement)?;
            hook_order.push(crate::domain::hooks::HookSlot::State(binding.name.clone()));
            states.push(binding);
            continue;
        }
        let Statement::VariableDeclaration(declaration) = statement else {
            return Err(err(
                statement,
                "only unconditional const id = useId() before return supported",
            ));
        };
        if !use_id
            || declaration.kind != VariableDeclarationKind::Const
            || declaration.declare
            || declaration.declarations.len() != 1
        {
            return Err(err(
                statement,
                "useId requires one const binding and named React import",
            ));
        }
        let binding = &declaration.declarations[0];
        let BindingPattern::BindingIdentifier(name) = &binding.id else {
            return Err(err(statement, "useId requires identifier binding"));
        };
        let Some(Expression::CallExpression(call)) = &binding.init else {
            return Err(err(statement, "only useId() initialization supported"));
        };
        if binding.type_annotation.is_some()
            || binding.definite
            || call.optional
            || call.type_arguments.is_some()
            || !call.arguments.is_empty()
            || !matches!(&call.callee,Expression::Identifier(i) if i.name == "useId")
            || !ids.insert(name.name.to_string())
        {
            return Err(err(statement, "plain unique const id = useId() required"));
        }
        hook_order.push(crate::domain::hooks::HookSlot::Id(name.name.to_string()));
    }
    let mut expr = ret
        .argument
        .as_ref()
        .ok_or_else(|| err(ret.as_ref(), "JSX return required"))?;
    while let Expression::ParenthesizedExpression(p) = expr {
        expr = &p.expression;
    }
    let Expression::JSXElement(root) = expr else {
        return Err(err(expr, "JSX element root required"));
    };
    let mut root = element(root)?;
    state::resolve(&mut root, &states);
    if use_state && states.is_empty() {
        return Err(err(f, "unused useState import"));
    }
    if crate::domain::hooks::ids(&root) != ids || (use_id && ids.is_empty()) {
        return Err(err(f, "unbound identifier or unused useId binding/import"));
    }
    fn uses(node: &Node, name: &str) -> bool {
        fn slot(value: &Value, name: &str) -> bool {
            matches!(value,Value::NodeSlot(node_slots::NodeSlot::Content(nodes)) if nodes.iter().any(|node|uses(node,name)))
        }
        match &node.kind {
            NodeKind::Text(value) if slot(value, name) => true,
            NodeKind::Text(Value::Repeat(repeat)) => uses(&repeat.template, name),
            NodeKind::Element {
                tag,
                children,
                attributes,
            } => {
                tag == name
                    || attributes
                        .iter()
                        .any(|attribute| slot(&attribute.value, name))
                    || children.iter().any(|child| uses(child, name))
            }
            _ => false,
        }
    }
    if imported.iter().any(|name| !uses(&root, name)) {
        return Err(err(
            f,
            "unused component dependency cannot be silently discarded",
        ));
    }
    Ok(Component {
        hook_order: if states.is_empty() || ids.is_empty() {
            Vec::new()
        } else {
            hook_order
        },
        states,
        client: !parsed.program.directives.is_empty(),
        name: f
            .id
            .as_ref()
            .ok_or_else(|| err(f, "named function required"))?
            .name
            .to_string(),
        props: props.ok_or_else(|| err(f, "Props alias required"))?,
        root,
    })
}

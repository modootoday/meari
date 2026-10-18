use crate::domain::{self, *};
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};

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
        Expression::StringLiteral(s) => Ok(Value::String(s.value.to_string())),
        Expression::BooleanLiteral(b) => Ok(Value::Bool(b.value)),
        Expression::Identifier(i) => Ok(Value::Id(i.name.to_string())),
        Expression::StaticMemberExpression(m)
            if !m.optional
                && matches!(&m.object, Expression::Identifier(i) if i.name == "props") =>
        {
            Ok(Value::Prop(m.property.name.to_string()))
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
        let value = match &a.value {
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
        attributes.push(Attribute {
            name: name.name.to_string(),
            value,
        });
    }
    let mut children = Vec::new();
    for child in &e.children {
        match child {
            JSXChild::Element(e) => children.push(element(e)?),
            JSXChild::ExpressionContainer(c)
                if matches!(c.expression, JSXExpression::EmptyExpression(_)) => {}
            JSXChild::ExpressionContainer(c) => children.push(Node {
                kind: NodeKind::Text(jsx_value(&c.expression)?),
                span: location(c.as_ref()),
            }),
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
    Ok(Node {
        kind: NodeKind::Element {
            tag,
            attributes,
            children,
        },
        span: location(e),
    })
}

pub fn parse(source: &str) -> Result<Component, Diagnostic> {
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
    for statement in &parsed.program.body {
        match statement {
            Statement::ImportDeclaration(import) => {
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
                        && name.name == "useId"
                        && specifier.local.name == "useId"
                        && specifier.import_kind == ImportOrExportKind::Value
                        && !use_id
                    {
                        use_id = true;
                        continue;
                    }
                    let module = crate::domain::design::components::module(name.name.as_str());
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
                    let kind = match &annotation.type_annotation {
                        TSType::TSStringKeyword(_) => PropType::String,
                        TSType::TSBooleanKeyword(_) => PropType::Bool,
                        _ => return Err(err(p.as_ref(), "prop type must be string/boolean")),
                    };
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
        || f.params.items.len() != 1
    {
        return Err(err(f, "plain synchronous function(props: Props) required"));
    }
    let p = &f.params.items[0];
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
    for statement in declarations {
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
    let root = element(root)?;
    if crate::domain::hooks::ids(&root) != ids || (use_id && ids.is_empty()) {
        return Err(err(f, "unbound identifier or unused useId binding/import"));
    }
    fn uses(node: &Node, name: &str) -> bool {
        matches!(&node.kind, NodeKind::Element { tag, children, .. } if tag == name || children.iter().any(|child| uses(child, name)))
    }
    if imported.iter().any(|name| !uses(&root, name)) {
        return Err(err(
            f,
            "unused component dependency cannot be silently discarded",
        ));
    }
    Ok(Component {
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

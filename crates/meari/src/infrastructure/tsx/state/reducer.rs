//! `useReducer` lowered to ordinary state: `const [n, dispatch] = useReducer(reducer, 0)`.
//!
//! The reducer must be a module-level `function reducer(state, action)` whose body selects one
//! returned expression by a literal action (`action === "x"`, `action.type === "x"`, via
//! `return c ? a : b` chains, `if (c) return a;` sequences or `switch`). Every `dispatch(literal)`
//! call is resolved at compile time to the selected branch and lowered to a state updater over
//! `state`, so both runtimes keep using the existing validated state program. Anything else
//! (actions carrying payloads, reducers reading free variables, non-literal dispatch) is
//! rejected instead of approximated.
use super::super::{err, location};
use crate::domain::{
    Diagnostic, Node, NodeKind, Value,
    component_state::StateValue,
    hook_expression::{Entry, Kind},
    node_slots::NodeSlot,
    state_program::{StateBinding, StateUpdate},
};
use oxc_ast::ast::*;
use oxc_syntax::operator::BinaryOperator;
use std::collections::BTreeMap;

/// A module-level reducer function, keyed by its name.
pub(in super::super) struct Reducer<'a> {
    function: &'a Function<'a>,
    state: String,
    action: String,
}

pub(in super::super) fn function<'a>(
    function: &'a Function<'a>,
    reducers: &mut BTreeMap<String, Reducer<'a>>,
) -> Result<(), Diagnostic> {
    let plain = |item: &FormalParameter<'_>| match &item.pattern {
        BindingPattern::BindingIdentifier(name)
            if item.initializer.is_none() && !item.optional && item.decorators.is_empty() =>
        {
            Some(name.name.to_string())
        }
        _ => None,
    };
    let [state, action] = function.params.items.as_slice() else {
        return Err(err(function, "reducer requires (state, action) parameters"));
    };
    let (Some(state), Some(action), Some(name)) = (plain(state), plain(action), &function.id)
    else {
        return Err(err(
            function,
            "reducer requires plain state/action identifiers",
        ));
    };
    if function.r#async
        || function.generator
        || function.declare
        || function.type_parameters.is_some()
        || function.params.rest.is_some()
        || function.body.is_none()
        || reducers.contains_key(name.name.as_str())
    {
        return Err(err(
            function,
            "plain unique synchronous reducer function required",
        ));
    }
    reducers.insert(
        name.name.to_string(),
        Reducer {
            function,
            state,
            action,
        },
    );
    Ok(())
}

pub(in super::super) fn is_declaration(statement: &Statement<'_>) -> bool {
    matches!(statement, Statement::VariableDeclaration(declaration)
        if declaration.declarations.iter().any(|binding| matches!(&binding.init,
            Some(Expression::CallExpression(call))
                if matches!(&call.callee, Expression::Identifier(name) if name.name == "useReducer"))))
}

/// `const [value, dispatch] = useReducer(reducer, literal)`; records `dispatch -> reducer`.
pub(in super::super) fn declaration(
    statement: &Statement<'_>,
    reducers: &BTreeMap<String, Reducer<'_>>,
    dispatchers: &mut BTreeMap<String, String>,
) -> Result<StateBinding, Diagnostic> {
    let Statement::VariableDeclaration(declaration) = statement else {
        return Err(err(statement, "useReducer declaration required"));
    };
    let fail = || {
        err(
            statement,
            "plain const [value, dispatch] = useReducer(reducer, literal) required",
        )
    };
    if declaration.kind != VariableDeclarationKind::Const
        || declaration.declare
        || declaration.declarations.len() != 1
    {
        return Err(fail());
    }
    let binding = &declaration.declarations[0];
    let BindingPattern::ArrayPattern(pattern) = &binding.id else {
        return Err(fail());
    };
    let (
        [
            Some(BindingPattern::BindingIdentifier(name)),
            Some(BindingPattern::BindingIdentifier(dispatch)),
        ],
        Some(Expression::CallExpression(call)),
    ) = (pattern.elements.as_slice(), &binding.init)
    else {
        return Err(fail());
    };
    let [reducer, initial] = call.arguments.as_slice() else {
        return Err(fail());
    };
    let Some(Expression::Identifier(reducer)) = reducer.as_expression() else {
        return Err(err(statement, "useReducer requires a named module reducer"));
    };
    if pattern.rest.is_some()
        || binding.type_annotation.is_some()
        || call.optional
        || call.type_arguments.is_some()
        || !reducers.contains_key(reducer.name.as_str())
        || dispatchers.contains_key(dispatch.name.as_str())
    {
        return Err(err(
            statement,
            "useReducer requires a declared module reducer and unique dispatch",
        ));
    }
    let initial = super::literal(initial.as_expression().ok_or_else(fail)?)?;
    dispatchers.insert(dispatch.name.to_string(), reducer.name.to_string());
    Ok(StateBinding {
        name: name.name.to_string(),
        setter: dispatch.name.to_string(),
        initial,
        span: location(statement),
    })
}

/// The literal tag a dispatch carries: a string action, or `{ type: "x" }`.
fn tag(update: &StateUpdate) -> Option<String> {
    match update {
        StateUpdate::Set(StateValue::String(text)) => Some(text.clone()),
        StateUpdate::Set(StateValue::Structured(expression)) => match &expression.kind {
            Kind::Record(entries) => match entries.as_slice() {
                [Entry::Field { key, value }] if key == "type" => match &value.kind {
                    Kind::Text(text) => Some(text.clone()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

fn unwrap<'b>(mut expression: &'b Expression<'b>) -> &'b Expression<'b> {
    while let Expression::ParenthesizedExpression(inner) = expression {
        expression = &inner.expression;
    }
    expression
}

/// `Some(true/false)` when `test` compares the action (or `action.type`) to a literal.
fn matches(test: &Expression<'_>, reducer: &Reducer<'_>, action: &str) -> Option<bool> {
    let Expression::BinaryExpression(binary) = unwrap(test) else {
        return None;
    };
    let negate = match binary.operator {
        BinaryOperator::StrictEquality => false,
        BinaryOperator::StrictInequality => true,
        _ => return None,
    };
    let subject = |expression: &Expression<'_>| match unwrap(expression) {
        Expression::Identifier(name) => name.name == reducer.action.as_str(),
        Expression::StaticMemberExpression(member) => {
            !member.optional
                && member.property.name == "type"
                && matches!(&member.object, Expression::Identifier(name) if name.name == reducer.action.as_str())
        }
        _ => false,
    };
    let text = |expression: &Expression<'_>| match unwrap(expression) {
        Expression::StringLiteral(text) => Some(text.value.to_string()),
        _ => None,
    };
    let literal = if subject(&binary.left) {
        text(&binary.right)
    } else if subject(&binary.right) {
        text(&binary.left)
    } else {
        None
    }?;
    Some((literal == action) != negate)
}

/// Selects the returned expression for a literal action; `None` means "not handled here".
fn select<'b>(
    statements: &'b [Statement<'b>],
    reducer: &Reducer<'_>,
    action: &str,
) -> Result<Option<&'b Expression<'b>>, Diagnostic> {
    for statement in statements {
        match statement {
            Statement::ReturnStatement(ret) => {
                let mut expression = unwrap(
                    ret.argument
                        .as_ref()
                        .ok_or_else(|| err(ret.as_ref(), "reducer return value required"))?,
                );
                while let Expression::ConditionalExpression(conditional) = expression {
                    match matches(&conditional.test, reducer, action) {
                        Some(true) => expression = unwrap(&conditional.consequent),
                        Some(false) => expression = unwrap(&conditional.alternate),
                        None => break,
                    }
                }
                return Ok(Some(expression));
            }
            Statement::IfStatement(branch) => {
                let chosen = match matches(&branch.test, reducer, action) {
                    Some(true) => Some(&branch.consequent),
                    Some(false) => branch.alternate.as_ref(),
                    None => {
                        return Err(err(
                            branch.as_ref(),
                            "reducer if requires a literal action test",
                        ));
                    }
                };
                if let Some(chosen) = chosen
                    && let Some(found) = select(std::slice::from_ref(chosen), reducer, action)?
                {
                    return Ok(Some(found));
                }
            }
            Statement::BlockStatement(block) => {
                if let Some(found) = select(&block.body, reducer, action)? {
                    return Ok(Some(found));
                }
            }
            Statement::SwitchStatement(switch) => {
                let subject = match unwrap(&switch.discriminant) {
                    Expression::Identifier(name) => name.name == reducer.action.as_str(),
                    Expression::StaticMemberExpression(member) => {
                        member.property.name == "type"
                            && matches!(&member.object, Expression::Identifier(name) if name.name == reducer.action.as_str())
                    }
                    _ => false,
                };
                if !subject {
                    return Err(err(
                        switch.as_ref(),
                        "reducer switch requires the action or action.type",
                    ));
                }
                let start = switch
                    .cases
                    .iter()
                    .position(|case| matches!(&case.test, Some(Expression::StringLiteral(text)) if text.value == action))
                    .or_else(|| switch.cases.iter().position(|case| case.test.is_none()));
                if let Some(test) = switch.cases.iter().find_map(|case| {
                    case.test
                        .as_ref()
                        .filter(|test| !matches!(test, Expression::StringLiteral(_)))
                }) {
                    return Err(err(test, "reducer case requires a string literal"));
                }
                // Fall-through: from the selected case, bodies run in order until a return/break.
                if let Some(start) = start {
                    for case in &switch.cases[start..] {
                        let mut broke = false;
                        for statement in &case.consequent {
                            if matches!(statement, Statement::BreakStatement(_)) {
                                broke = true;
                                break;
                            }
                            if let Some(found) =
                                select(std::slice::from_ref(statement), reducer, action)?
                            {
                                return Ok(Some(found));
                            }
                        }
                        if broke {
                            break;
                        }
                    }
                }
            }
            Statement::ThrowStatement(throw) => {
                return Err(err(
                    throw.as_ref(),
                    "dispatched action reaches a reducer throw",
                ));
            }
            Statement::BreakStatement(_) => return Ok(None),
            Statement::EmptyStatement(_) => {}
            _ => {
                return Err(err(
                    statement,
                    "reducer body supports only return/if/switch on the action",
                ));
            }
        }
    }
    Ok(None)
}

fn lower(update: &mut StateUpdate, reducer: &Reducer<'_>) -> Result<(), Diagnostic> {
    let function = reducer.function;
    let action = tag(update).ok_or_else(|| {
        err(
            function,
            "dispatch requires a string or { type } literal action",
        )
    })?;
    let body = function
        .body
        .as_ref()
        .ok_or_else(|| err(function, "reducer body required"))?;
    let selected = select(&body.statements, reducer, &action)?
        .ok_or_else(|| err(function, "dispatched action has no reducer return"))?;
    *update = super::captured(selected, Some(reducer.state.as_str()))?;
    Ok(())
}

/// Rewrites every dispatch event to the reducer branch it selects.
pub(in super::super) fn resolve(
    node: &mut Node,
    reducers: &BTreeMap<String, Reducer<'_>>,
    dispatchers: &BTreeMap<String, String>,
) -> Result<(), Diagnostic> {
    let mut convert = |value: &mut Value| -> Result<(), Diagnostic> {
        if let Value::Event(event) = value
            && let Some(reducer) = dispatchers
                .get(&event.setter)
                .and_then(|name| reducers.get(name))
        {
            lower(&mut event.update, reducer)?;
        }
        Ok(())
    };
    fn walk(
        node: &mut Node,
        convert: &mut dyn FnMut(&mut Value) -> Result<(), Diagnostic>,
    ) -> Result<(), Diagnostic> {
        match &mut node.kind {
            NodeKind::Text(Value::Repeat(repeat)) => walk(&mut repeat.template, convert),
            NodeKind::Text(Value::NodeSlot(NodeSlot::Content(nodes))) => {
                nodes.iter_mut().try_for_each(|child| walk(child, convert))
            }
            NodeKind::Text(value) => convert(value),
            NodeKind::Element {
                attributes,
                children,
                ..
            } => {
                for attribute in attributes {
                    convert(&mut attribute.value)?;
                    if let Value::NodeSlot(NodeSlot::Content(nodes)) = &mut attribute.value {
                        nodes
                            .iter_mut()
                            .try_for_each(|child| walk(child, convert))?;
                    }
                }
                children
                    .iter_mut()
                    .try_for_each(|child| walk(child, convert))
            }
        }
    }
    walk(node, &mut convert)
}

use crate::domain::{
    component_state::StateValue,
    state_expression::{Binary, StateExpression as E, Unary},
};

pub(super) fn emit(expression: &E) -> String {
    emit_in(expression, "previous")
}

pub(super) fn updater(expression: &E) -> String {
    fn references(expression: &E, name: &str) -> bool {
        match expression {
            E::State { name: state } => state == name,
            E::Unary { argument, .. } => references(argument, name),
            E::Binary { left, right, .. } => references(left, name) || references(right, name),
            E::Conditional {
                test,
                consequent,
                alternate,
            } => {
                references(test, name)
                    || references(consequent, name)
                    || references(alternate, name)
            }
            _ => false,
        }
    }
    let mut previous = "previous".to_owned();
    while references(expression, &previous) {
        previous.push('_');
    }
    format!("{previous} => {}", emit_in(expression, &previous))
}

fn emit_in(expression: &E, previous: &str) -> String {
    match expression {
        E::Literal { value } => match value {
            StateValue::Bool(value) => value.to_string(),
            StateValue::String(value) => serde_json::to_string(value).unwrap(),
            StateValue::Number(value) => value.to_string(),
            StateValue::Json(value) => value.to_string(),
            StateValue::Structured(value) => super::hook_expression::emit(value),
        },
        E::State { name } => name.clone(),
        E::Previous => previous.into(),
        E::Unary { operator, argument } => format!(
            "({}({}))",
            match operator {
                Unary::Not => "!",
                Unary::Negate => "-",
            },
            emit_in(argument, previous)
        ),
        E::Binary {
            operator,
            left,
            right,
        } => format!(
            "({} {} {})",
            emit_in(left, previous),
            match operator {
                Binary::Add => "+",
                Binary::Subtract => "-",
                Binary::Multiply => "*",
                Binary::Divide => "/",
                Binary::Remainder => "%",
                Binary::Equal => "===",
                Binary::NotEqual => "!==",
                Binary::Less => "<",
                Binary::LessEqual => "<=",
                Binary::Greater => ">",
                Binary::GreaterEqual => ">=",
                Binary::And => "&&",
                Binary::Or => "||",
            },
            emit_in(right, previous)
        ),
        E::Conditional {
            test,
            consequent,
            alternate,
        } => format!(
            "({} ? {} : {})",
            emit_in(test, previous),
            emit_in(consequent, previous),
            emit_in(alternate, previous)
        ),
    }
}

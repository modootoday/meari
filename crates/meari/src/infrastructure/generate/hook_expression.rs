use crate::domain::hook_expression::{Entry, Expression, Kind};

pub(super) fn emit(expression: &Expression) -> String {
    emit_in(expression, "previous")
}

pub(super) fn updater(expression: &Expression) -> String {
    fn mentions(expression: &Expression, name: &str) -> bool {
        match &expression.kind {
            Kind::State(value) => value == name,
            Kind::Field { target, .. } | Kind::Element { target, .. } | Kind::Not(target) => {
                mentions(target, name)
            }
            Kind::Array(items) => items.iter().any(|item| mentions(item, name)),
            Kind::Record(entries) => entries.iter().any(|entry| {
                let (Entry::Field { value, .. } | Entry::Spread { value }) = entry;
                mentions(value, name)
            }),
            Kind::Add { left, right } => mentions(left, name) || mentions(right, name),
            _ => false,
        }
    }
    let mut previous = "previous".to_owned();
    while mentions(expression, &previous) {
        previous.push('_');
    }
    format!("{previous} => {}", emit_in(expression, &previous))
}

fn emit_in(expression: &Expression, previous: &str) -> String {
    match &expression.kind {
        Kind::Undefined => "undefined".into(),
        Kind::Null => "null".into(),
        Kind::Boolean(value) => value.to_string(),
        Kind::Number(bits) => {
            let value = f64::from_bits(*bits);
            if value == 0.0 && value.is_sign_negative() {
                "-0".into()
            } else {
                ryu_js::Buffer::new().format(value).to_owned()
            }
        }
        Kind::Text(value) => serde_json::to_string(value).unwrap(),
        Kind::State(name) => name.clone(),
        Kind::Previous => previous.into(),
        Kind::Field { target, key } => format!(
            "({})[{}]",
            emit_in(target, previous),
            serde_json::to_string(key).unwrap()
        ),
        Kind::Element { target, index } => format!("({})[{index}]", emit_in(target, previous)),
        Kind::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(|item| emit_in(item, previous))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Kind::Record(entries) => format!(
            "({{{}}})",
            entries
                .iter()
                .map(|entry| match entry {
                    Entry::Field { key, value } => format!(
                        "{}: {}",
                        serde_json::to_string(key).unwrap(),
                        emit_in(value, previous)
                    ),
                    Entry::Spread { value } => format!("...({})", emit_in(value, previous)),
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Kind::Not(value) => format!("!({})", emit_in(value, previous)),
        Kind::Add { left, right } => format!(
            "({} + {})",
            emit_in(left, previous),
            emit_in(right, previous)
        ),
    }
}

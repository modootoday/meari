//! Static source IR. Number bits preserve signed zero; no live heap identity is serialized.
use super::{Diagnostic, Span};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub(crate) mod shape;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum LiteralShape {
    Null,
    Boolean,
    Number,
    Text,
    Array(Vec<LiteralShape>),
    Record(BTreeMap<String, LiteralShape>),
}
impl LiteralShape {
    pub fn accepts(&self, actual: &Self) -> Result<bool, &'static str> {
        fn check(
            expected: &LiteralShape,
            actual: &LiteralShape,
            depth: usize,
            work: &mut usize,
        ) -> Result<bool, &'static str> {
            if depth > 64 || *work == 0 {
                return Err("structured literal type comparison budget exceeded");
            }
            *work -= 1;
            Ok(match (expected, actual) {
                (LiteralShape::Array(allowed), LiteralShape::Array(items)) => {
                    for item in items {
                        let mut found = false;
                        for expected in allowed {
                            if check(expected, item, depth + 1, work)? {
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            return Ok(false);
                        }
                    }
                    true
                }
                (LiteralShape::Record(expected), LiteralShape::Record(actual)) => {
                    if expected.len() != actual.len() {
                        return Ok(false);
                    }
                    for (key, shape) in expected {
                        let Some(actual) = actual.get(key) else {
                            return Ok(false);
                        };
                        if !check(shape, actual, depth + 1, work)? {
                            return Ok(false);
                        }
                    }
                    true
                }
                (LiteralShape::Null, LiteralShape::Null)
                | (LiteralShape::Boolean, LiteralShape::Boolean)
                | (LiteralShape::Number, LiteralShape::Number)
                | (LiteralShape::Text, LiteralShape::Text) => true,
                _ => false,
            })
        }
        check(self, actual, 0, &mut 65_536)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expression {
    pub span: Span,
    pub kind: Kind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Kind {
    Undefined,
    Null,
    Boolean(bool),
    Number(#[serde(with = "number_bits")] u64),
    Text(String),
    State(String),
    Previous,
    Field {
        target: Box<Expression>,
        key: String,
    },
    Element {
        target: Box<Expression>,
        index: usize,
    },
    Array(Vec<Expression>),
    Record(Vec<Entry>),
    Not(Box<Expression>),
    Add {
        left: Box<Expression>,
        right: Box<Expression>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Entry {
    Field { key: String, value: Expression },
    Spread { value: Expression },
}

// Hex text survives JSON transports that otherwise round integers above 2^53.
mod number_bits {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(bits: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{bits:016x}"))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text.len() != 16 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(serde::de::Error::custom(
                "16 hexadecimal number-bit digits required",
            ));
        }
        u64::from_str_radix(&text, 16).map_err(serde::de::Error::custom)
    }
}

impl Expression {
    pub fn literal_shape(&self) -> Result<LiteralShape, Diagnostic> {
        self.validate_literal()?;
        fn shape(expr: &Expression) -> LiteralShape {
            match &expr.kind {
                Kind::Null => LiteralShape::Null,
                Kind::Boolean(_) => LiteralShape::Boolean,
                Kind::Number(_) => LiteralShape::Number,
                Kind::Text(_) => LiteralShape::Text,
                Kind::Array(items) => {
                    let mut kinds = items.iter().map(shape).collect::<Vec<_>>();
                    kinds.sort();
                    kinds.dedup();
                    LiteralShape::Array(kinds)
                }
                Kind::Record(entries) => LiteralShape::Record(
                    entries
                        .iter()
                        .map(|entry| {
                            let Entry::Field { key, value } = entry else {
                                unreachable!("validated literal")
                            };
                            (key.clone(), shape(value))
                        })
                        .collect(),
                ),
                _ => unreachable!("validated literal"),
            }
        }
        Ok(shape(self))
    }
    pub fn erase_spans(&mut self) {
        self.span = Span::default();
        match &mut self.kind {
            Kind::Field { target, .. } | Kind::Element { target, .. } | Kind::Not(target) => {
                target.erase_spans()
            }
            Kind::Array(items) => items.iter_mut().for_each(Self::erase_spans),
            Kind::Record(entries) => {
                for entry in entries {
                    match entry {
                        Entry::Field { value, .. } | Entry::Spread { value } => value.erase_spans(),
                    }
                }
            }
            Kind::Add { left, right } => {
                left.erase_spans();
                right.erase_spans();
            }
            _ => (),
        }
    }
    /// Source declaration subset currently accepted by both real frontends.
    pub fn validate_literal(&self) -> Result<(), Diagnostic> {
        self.validate()?;
        fn data(expr: &Expression) -> Result<(), Diagnostic> {
            match &expr.kind {
                Kind::Null | Kind::Boolean(_) | Kind::Number(_) | Kind::Text(_) => (),
                Kind::Array(items) => {
                    for item in items {
                        data(item)?;
                    }
                }
                Kind::Record(entries) => {
                    for entry in entries {
                        let Entry::Field { value, .. } = entry else {
                            return Err(Diagnostic::unsupported(
                                expr.span,
                                "source object spread requires a captured updater context",
                            ));
                        };
                        data(value)?;
                    }
                }
                _ => {
                    return Err(Diagnostic::unsupported(
                        expr.span,
                        "plain structured data literal required",
                    ));
                }
            }
            Ok(())
        }
        if !matches!(self.kind, Kind::Array(_) | Kind::Record(_)) {
            return Err(Diagnostic::unsupported(
                self.span,
                "structured state requires an object or dense array",
            ));
        }
        data(self)
    }
    pub fn validate(&self) -> Result<(), Diagnostic> {
        fn walk(
            expr: &Expression,
            depth: usize,
            nodes: &mut usize,
            bytes: &mut usize,
        ) -> Result<(), Diagnostic> {
            *nodes += 1;
            if depth > 64 || *nodes > 16_384 {
                return Err(Diagnostic::unsupported(
                    expr.span,
                    "structured source expression budget exceeded",
                ));
            }
            fn text(value: &str, span: Span, bytes: &mut usize) -> Result<(), Diagnostic> {
                *bytes = bytes.checked_add(value.len()).ok_or_else(|| {
                    Diagnostic::unsupported(span, "structured source byte overflow")
                })?;
                if *bytes > 1024 * 1024 {
                    return Err(Diagnostic::unsupported(
                        span,
                        "structured source text budget exceeded",
                    ));
                }
                Ok(())
            }
            match &expr.kind {
                Kind::Number(bits) if !f64::from_bits(*bits).is_finite() => {
                    return Err(Diagnostic::unsupported(
                        expr.span,
                        "finite source number required",
                    ));
                }
                Kind::Text(value) => text(value, expr.span, bytes)?,
                Kind::State(name) => {
                    text(name, expr.span, bytes)?;
                    if !super::validation::identifier(name) {
                        return Err(Diagnostic::unsupported(
                            expr.span,
                            "structured state binding name required",
                        ));
                    }
                }
                Kind::Field { target, key } => {
                    text(key, expr.span, bytes)?;
                    walk(target, depth + 1, nodes, bytes)?;
                }
                Kind::Element { target, .. } | Kind::Not(target) => {
                    walk(target, depth + 1, nodes, bytes)?
                }
                Kind::Array(items) => {
                    for item in items {
                        walk(item, depth + 1, nodes, bytes)?;
                    }
                }
                Kind::Record(entries) => {
                    for entry in entries {
                        let value = match entry {
                            Entry::Field { key, value } => {
                                if key == "__proto__" {
                                    return Err(Diagnostic::unsupported(
                                        value.span,
                                        "prototype-setting object literals are unsupported",
                                    ));
                                }
                                *bytes = bytes.checked_add(key.len()).ok_or_else(|| {
                                    Diagnostic::unsupported(
                                        expr.span,
                                        "structured source byte overflow",
                                    )
                                })?;
                                if *bytes > 1024 * 1024 {
                                    return Err(Diagnostic::unsupported(
                                        expr.span,
                                        "structured source text budget exceeded",
                                    ));
                                }
                                value
                            }
                            Entry::Spread { value } => value,
                        };
                        walk(value, depth + 1, nodes, bytes)?;
                    }
                }
                Kind::Add { left, right } => {
                    walk(left, depth + 1, nodes, bytes)?;
                    walk(right, depth + 1, nodes, bytes)?;
                }
                _ => (),
            }
            Ok(())
        }
        walk(self, 0, &mut 0, &mut 0)
    }
}

//! Internal immutable values; storage equality and hook comparison are separate policies.
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    Undefined,
    Null,
    Boolean,
    Number,
    String,
    Record,
    Array,
}

pub(crate) enum Data {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Record(BTreeMap<String, u64>),
    Array(Vec<u64>),
}

pub(crate) struct Node {
    pub data: Data,
    pub depth: usize,
}

impl Data {
    pub fn kind(&self) -> ValueKind {
        match self {
            Self::Undefined => ValueKind::Undefined,
            Self::Null => ValueKind::Null,
            Self::Boolean(_) => ValueKind::Boolean,
            Self::Number(_) => ValueKind::Number,
            Self::String(_) => ValueKind::String,
            Self::Record(_) => ValueKind::Record,
            Self::Array(_) => ValueKind::Array,
        }
    }
}

pub(crate) fn same_value(left: &Data, right: &Data, same_identity: bool) -> bool {
    match (left, right) {
        (Data::Undefined, Data::Undefined) | (Data::Null, Data::Null) => true,
        (Data::Boolean(a), Data::Boolean(b)) => a == b,
        (Data::Number(a), Data::Number(b)) => {
            (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
        }
        (Data::String(a), Data::String(b)) => a == b,
        (Data::Record(_), Data::Record(_)) | (Data::Array(_), Data::Array(_)) => same_identity,
        _ => false,
    }
}

//! Bounded own-field reads from declared immutable records.
use super::super::{Prop, PropType, Value};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberValue {
    pub root: Box<Value>,
    pub path: Vec<String>,
}

impl MemberValue {
    pub fn field_type(mut kind: PropType, path: &[String]) -> Result<PropType, String> {
        if path.is_empty() || path.len() > 32 {
            return Err("record member path exceeds its depth budget".into());
        }
        kind.validate()?;
        for name in path {
            if !super::super::validation::identifier(name) {
                return Err("record member requires a portable field name".into());
            }
            let PropType::Record(fields) = kind else {
                return Err("member access requires a declared record".into());
            };
            kind = fields
                .into_iter()
                .find(|field| field.name == *name)
                .ok_or_else(|| format!("unknown record field: {name}"))?
                .kind;
        }
        Ok(kind)
    }

    pub fn kind(&self, props: &[Prop]) -> Result<PropType, String> {
        let kind = match self.root.as_ref() {
            Value::Prop(name) => props
                .iter()
                .find(|prop| prop.name == *name)
                .ok_or_else(|| format!("unknown record prop: {name}"))?
                .kind
                .clone(),
            Value::Record(record) => {
                record.validate()?;
                record.kind()
            }
            _ => {
                return Err(
                    "record member requires its declared prop or lexical repeat scope".into(),
                );
            }
        };
        Self::field_type(kind, &self.path)
    }

    pub fn bound(&self) -> Result<Option<Value>, String> {
        let Value::Record(record) = self.root.as_ref() else {
            return match self.root.as_ref() {
                Value::Prop(_) | Value::Local(_) => Ok(None),
                _ => Err("record member root is not a bound record".into()),
            };
        };
        Self::field_type(record.kind(), &self.path)?;
        record.validate()?;
        let mut value = self.root.as_ref();
        for name in &self.path {
            let Value::Record(record) = value else {
                return Err("record member traverses a non-record value".into());
            };
            value = record
                .get(name)
                .ok_or_else(|| format!("missing record field: {name}"))?;
        }
        Ok(Some(value.clone()))
    }
}

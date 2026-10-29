//! Validated immutable host input; never a substitute for live hook object identity.
use super::super::{Prop, PropType, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(in super::super) fn same_schema(left: &[Prop], right: &[Prop]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    fn fields(schema: &[Prop]) -> BTreeMap<&str, Vec<&PropType>> {
        let mut fields: BTreeMap<&str, Vec<&PropType>> = BTreeMap::new();
        for field in schema {
            fields.entry(&field.name).or_default().push(&field.kind);
        }
        fields
    }
    fields(left) == fields(right)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordValue {
    pub schema: Vec<Prop>,
    pub fields: BTreeMap<String, Value>,
}

impl RecordValue {
    pub fn new(schema: Vec<Prop>, fields: BTreeMap<String, Value>) -> Result<Self, String> {
        let value = Self { schema, fields };
        value.validate()?;
        Ok(value)
    }

    pub fn schema(&self) -> &[Prop] {
        &self.schema
    }

    pub fn fields(&self) -> &BTreeMap<String, Value> {
        &self.fields
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.fields.get(name)
    }

    pub fn kind(&self) -> PropType {
        PropType::Record(self.schema.clone())
    }

    /// Deserialization is intentionally followed by the same validation as host binding.
    pub fn validate(&self) -> Result<(), String> {
        super::super::prop_values::validate_record(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::array_value::ArrayValue;

    fn schema() -> Vec<Prop> {
        vec![
            Prop {
                name: "key".into(),
                kind: PropType::String,
            },
            Prop {
                name: "label".into(),
                kind: PropType::String,
            },
        ]
    }

    fn row(key: &str, label: &str) -> RecordValue {
        RecordValue::new(
            schema(),
            BTreeMap::from([
                ("key".into(), Value::String(key.into())),
                ("label".into(), Value::String(label.into())),
            ]),
        )
        .unwrap()
    }

    #[test]
    fn empty_and_ordered_record_arrays_keep_the_declared_shape() {
        let kind = PropType::Record(schema());
        let empty = ArrayValue {
            element: kind.clone(),
            items: vec![],
        };
        empty.validate().unwrap();
        let values = ArrayValue {
            element: kind.clone(),
            items: vec![
                Value::Record(row("second", "두 번째🦀")),
                Value::Record(row("first", "첫 번째")),
            ],
        };
        values.validate().unwrap();
        assert!(PropType::Array(Box::new(kind)).accepts(&Value::Array(values.clone())));
        let restored: ArrayValue =
            serde_json::from_str(&serde_json::to_string(&values).unwrap()).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored, values);
        let Value::Record(first) = &restored.items[0] else {
            panic!("record expected")
        };
        assert_eq!(first.get("key"), Some(&Value::String("second".into())));
        let mut reordered = schema();
        reordered.reverse();
        assert!(PropType::Record(reordered).accepts(&Value::Record(row("a", "A"))));
    }

    #[test]
    fn missing_extra_duplicate_wrong_and_forged_fields_are_rejected() {
        let valid = row("a", "A");
        let mut missing = valid.fields().clone();
        missing.remove("key");
        assert!(RecordValue::new(schema(), missing).is_err());
        let mut extra = valid.fields().clone();
        extra.insert("hidden".into(), Value::Bool(true));
        assert!(RecordValue::new(schema(), extra).is_err());
        let mut wrong = valid.fields().clone();
        wrong.insert("key".into(), Value::Bool(true));
        assert!(RecordValue::new(schema(), wrong).is_err());
        let mut duplicate = schema();
        duplicate.push(duplicate[0].clone());
        assert!(RecordValue::new(duplicate, valid.fields().clone()).is_err());
        let mut wire = serde_json::to_value(valid).unwrap();
        wire["fields"]["key"] = serde_json::json!({"Bool": true});
        let forged: RecordValue = serde_json::from_value(wire).unwrap();
        assert!(!PropType::Record(schema()).accepts(&Value::Record(forged)));
    }

    #[test]
    fn alternating_record_array_containers_share_one_global_budget() {
        let small = row("a", &"x".repeat(600_000));
        let oversized = ArrayValue {
            element: small.kind(),
            items: vec![Value::Record(small.clone()), Value::Record(small)],
        };
        assert!(oversized.validate().is_err());
        let mut value = Value::String("leaf".into());
        let mut kind = PropType::String;
        for depth in 0..34 {
            if depth % 2 == 0 {
                value = Value::Array(ArrayValue {
                    element: kind.clone(),
                    items: vec![value],
                });
                kind = PropType::Array(Box::new(kind));
            } else {
                let fields = vec![Prop {
                    name: "next".into(),
                    kind,
                }];
                // Deserialize to model an untrusted binding that bypassed the constructor.
                let wire = serde_json::json!({"schema": fields, "fields": {"next": value}});
                let record: RecordValue = serde_json::from_value(wire).unwrap();
                kind = record.kind();
                value = Value::Record(record);
            }
        }
        assert!(kind.validate().is_err());
        assert!(!kind.accepts(&value));
    }
}

//! Deterministic native declarations for structurally typed host record props.
use super::{Language, prop_type};
use crate::domain::{Prop, PropType, build::ContentHash};
use std::{collections::BTreeMap, fmt::Write};

pub(crate) fn name(fields: &[Prop]) -> String {
    let bytes = serde_json::to_vec(fields).expect("record schemas serialize infallibly");
    format!("MeariRecord{}", ContentHash::of(&bytes).as_str())
}

fn schema(fields: &[Prop]) -> String {
    let mut result = String::from("vec![");
    for field in fields {
        write!(
            result,
            "meari::domain::Prop {{ name: {:?}.into(), kind: <{} as PropElement>::kind(), }},",
            field.name,
            prop_type(&field.kind, Language::Rust)
        )
        .unwrap();
    }
    result.push(']');
    result
}

pub(crate) fn implementations(fields: &[Prop], name: &str) -> String {
    let mut code = format!(
        "impl PropElement for {name} {{\n    fn kind() -> meari::domain::PropType {{\n        meari::domain::PropType::Record({})\n    }}\n}}\n",
        schema(fields)
    );
    writeln!(code, "impl IntoValue for {name} {{\n    fn into_value(self) -> meari::domain::Value {{\n        meari::domain::Value::Record(meari::domain::record_value::RecordValue {{\n            schema: {},\n            fields: std::collections::BTreeMap::from([", schema(fields)).unwrap();
    for field in fields {
        writeln!(
            code,
            "                ({:?}.into(), self.{}.into_value()),",
            field.name, field.name
        )
        .unwrap();
    }
    code.push_str("            ]),\n        })\n    }\n}\n");
    code
}

pub(super) fn emit(props: &[Prop], code: &mut String) {
    fn collect(kind: &PropType, records: &mut BTreeMap<String, Vec<Prop>>) {
        match kind {
            PropType::Array(element) => collect(element, records),
            PropType::Record(fields) => {
                records.insert(name(fields), fields.clone());
                for field in fields {
                    collect(&field.kind, records);
                }
            }
            _ => {}
        }
    }
    let mut records = BTreeMap::new();
    for prop in props {
        collect(&prop.kind, &mut records);
    }
    for (name, fields) in records {
        code.push_str("#[derive(Clone)]\n#[allow(non_snake_case)]\n");
        writeln!(code, "pub struct {name} {{").unwrap();
        for field in &fields {
            writeln!(
                code,
                "    pub {}: {},",
                field.name,
                prop_type(&field.kind, Language::Rust)
            )
            .unwrap();
        }
        code.push_str("}\n");
        code.push_str(&implementations(&fields, &name));
        code.push('\n');
    }
}

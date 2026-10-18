use crate::application::{Language, Mapping, Output};
use crate::domain::*;
use std::fmt::Write;

fn value(value: &Value, language: Language, props: &[Prop]) -> String {
    match value {
        Value::String(s) => match language {
            Language::Rust => format!("{s:?}"),
            Language::Tsx => serde_json::to_string(s).expect("string serialization is infallible"),
        },
        Value::Bool(b) => b.to_string(),
        Value::Id(name) => match language {
            Language::Tsx => name.clone(),
            Language::Rust => format!("use_id({name:?})"),
        },
        Value::Prop(p) => match language {
            Language::Rust
                if props
                    .iter()
                    .any(|prop| prop.name == *p && prop.kind == PropType::String) =>
            {
                format!("props.{p}.clone()")
            }
            _ => format!("props.{p}"),
        },
    }
}

fn node(
    n: &Node,
    language: Language,
    props: &[Prop],
    code: &mut String,
    mappings: &mut Vec<Mapping>,
) {
    let start = code.len();
    match (&n.kind, language) {
        (NodeKind::Text(v), Language::Rust) => {
            write!(code, "text({})", value(v, language, props)).unwrap();
        }
        (NodeKind::Text(v), Language::Tsx) => {
            write!(code, "{{{}}}", value(v, language, props)).unwrap();
        }
        (
            NodeKind::Element {
                tag,
                attributes,
                children,
            },
            Language::Rust,
        ) => {
            write!(code, "element({tag:?}, vec![").unwrap();
            for a in attributes {
                write!(
                    code,
                    "attribute({:?}, {}),",
                    a.name,
                    value(&a.value, language, props)
                )
                .unwrap();
            }
            code.push_str("], vec![");
            for child in children {
                node(child, language, props, code, mappings);
                code.push(',');
            }
            code.push_str("])");
        }
        (
            NodeKind::Element {
                tag,
                attributes,
                children,
            },
            Language::Tsx,
        ) => {
            write!(code, "<{tag}").unwrap();
            for a in attributes {
                write!(code, " {}={{{}}}", a.name, value(&a.value, language, props)).unwrap();
            }
            code.push('>');
            for child in children {
                node(child, language, props, code, mappings);
            }
            write!(code, "</{tag}>").unwrap();
        }
    }
    mappings.push(Mapping {
        source: n.span,
        generated: Span {
            start,
            end: code.len(),
        },
    });
}

pub fn emit(component: &Component, language: Language) -> Output {
    let mut code = String::new();
    if component.client {
        code.push_str(match language {
            Language::Tsx => "\"use client\";\n",
            Language::Rust => "pub const BRIDGE_USE_CLIENT: bool = true;\n",
        });
    }
    if matches!(language, Language::Tsx) {
        if !crate::domain::hooks::ids(&component.root).is_empty() {
            code.push_str("import { useId } from \"react\";\n");
        }
        fn imports(
            node: &Node,
            names: &mut std::collections::BTreeMap<
                &'static str,
                std::collections::BTreeSet<String>,
            >,
        ) {
            if let NodeKind::Element { tag, children, .. } = &node.kind {
                if let Some(module) = crate::domain::design::components::module(tag) {
                    names.entry(module).or_default().insert(tag.clone());
                }
                for child in children {
                    imports(child, names);
                }
            }
        }
        let mut names = std::collections::BTreeMap::new();
        imports(&component.root, &mut names);
        for (module, names) in names {
            writeln!(
                code,
                "import {{ {} }} from \"@/components/ui/{module}\";",
                names.into_iter().collect::<Vec<_>>().join(", ")
            )
            .unwrap();
        }
    }
    match language {
        Language::Rust => {
            code.push_str("use meari::runtime::*;\n\npub struct Props {\n");
            for p in &component.props {
                writeln!(
                    code,
                    "    pub {}: {},",
                    p.name,
                    match p.kind {
                        PropType::String => "String",
                        PropType::Bool => "bool",
                    }
                )
                .unwrap();
            }
            writeln!(
                code,
                "}}\n\n#[allow(non_snake_case)]\npub fn {}(props: Props) -> Node {{",
                component.name
            )
            .unwrap();
            code.push_str("    ");
        }
        Language::Tsx => {
            code.push_str("type Props = {\n");
            for p in &component.props {
                writeln!(
                    code,
                    "  {}: {};",
                    p.name,
                    match p.kind {
                        PropType::String => "string",
                        PropType::Bool => "boolean",
                    }
                )
                .unwrap();
            }
            writeln!(
                code,
                "}};\n\nexport function {}(props: Props) {{",
                component.name
            )
            .unwrap();
            for name in crate::domain::hooks::ids(&component.root) {
                writeln!(code, "  const {name} = useId();").unwrap();
            }
            code.push_str("  return (");
        }
    }
    let mut mappings = Vec::new();
    node(
        &component.root,
        language,
        &component.props,
        &mut code,
        &mut mappings,
    );
    code.push_str(match language {
        Language::Rust => "\n}\n",
        Language::Tsx => ");\n}\n",
    });
    Output {
        schema_version: 2,
        code,
        mappings,
        design_system: crate::domain::design::analyze(component),
    }
}

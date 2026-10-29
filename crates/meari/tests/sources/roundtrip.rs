use meari::{self as application, application::Language, domain::*};

const RUST: &str = include_str!("../../fixtures/card.rs");
const TSX: &str = include_str!("../../fixtures/card.tsx");

#[test]
fn independent_frontends_agree_and_both_roundtrip() {
    let rust = application::parse(RUST, Language::Rust).unwrap();
    let tsx = application::parse(TSX, Language::Tsx).unwrap();
    assert!(rust.semantic_eq(&tsx));
    for (source, from, to) in [
        (RUST, Language::Rust, Language::Tsx),
        (TSX, Language::Tsx, Language::Rust),
    ] {
        let original = application::parse(source, from).unwrap();
        let generated = application::convert(source, from, to).unwrap();
        let intermediate = application::parse(&generated.code, to).unwrap();
        assert!(original.semantic_eq(&intermediate), "{}", generated.code);
        let back = application::emit(&intermediate, from).unwrap();
        assert!(original.semantic_eq(&application::parse(&back.code, from).unwrap()));
    }
}

#[test]
fn escaping_unicode_entities_and_control_characters_survive() {
    for value in [
        "한글 🦀",
        "a\n\t\r\0b",
        "\"'&<>}{\\",
        "&amp;",
        "\u{2028}\u{2029}",
    ] {
        let mut ir = application::parse(TSX, Language::Tsx).unwrap();
        ir.root.kind = NodeKind::Element {
            tag: "div".into(),
            attributes: vec![Attribute {
                name: "title".into(),
                value: Value::String(value.into()),
            }],
            children: vec![Node {
                kind: NodeKind::Text(Value::String(value.into())),
                span: Span::default(),
            }],
        };
        for language in [Language::Rust, Language::Tsx] {
            let output = application::emit(&ir, language).unwrap();
            assert!(
                ir.semantic_eq(&application::parse(&output.code, language).unwrap()),
                "{}",
                output.code
            );
        }
    }
    let entities = "type Props = {}; export function Card(props: Props) { return <div title=\"&amp;\">&lt;&amp;</div>; }";
    let ir = application::parse(entities, Language::Tsx).unwrap();
    let NodeKind::Element {
        attributes,
        children,
        ..
    } = ir.root.kind
    else {
        panic!()
    };
    assert_eq!(attributes[0].value, Value::String("&".into()));
    assert_eq!(children[0].kind, NodeKind::Text(Value::String("<&".into())));
}

#[test]
fn mappings_point_to_exact_original_and_generated_nodes() {
    for (source, from, to) in [
        (RUST, Language::Rust, Language::Tsx),
        (TSX, Language::Tsx, Language::Rust),
    ] {
        let output = application::convert(source, from, to).unwrap();
        assert_eq!(output.mappings.len(), 4);
        for m in &output.mappings {
            assert!(!source.get(m.source.start..m.source.end).unwrap().is_empty());
            assert!(
                !output
                    .code
                    .get(m.generated.start..m.generated.end)
                    .unwrap()
                    .is_empty()
            );
        }
        let text = &output.mappings[0];
        assert!(source[text.source.start..text.source.end].contains("props.label"));
    }
    let unicode = TSX.replace(
        ">{props.label}</button>",
        ">{\"한글\"}{props.label}</button>",
    );
    let out = application::convert(&unicode, Language::Tsx, Language::Rust).unwrap();
    for m in out.mappings {
        assert!(unicode.get(m.source.start..m.source.end).is_some());
    }
    let unicode_rust = RUST.replace(
        "text(props.label.clone())",
        "text(\"한글🦀\"), text(props.label.clone())",
    );
    let out = application::convert(&unicode_rust, Language::Rust, Language::Tsx).unwrap();
    assert!(
        out.mappings
            .iter()
            .any(|m| unicode_rust.get(m.source.start..m.source.end) == Some("text(\"한글🦀\")"))
    );
    assert!(out.mappings.iter().any(
        |m| unicode_rust.get(m.source.start..m.source.end) == Some("text(props.label.clone())")
    ));
}

#[test]
fn unsupported_tsx_never_silently_discards_behavior() {
    for body in [
        "const [n, setN] = useState(0); return <div />;",
        "useEffect(() => alert('x')); return <div />;",
        "return <div onClick={() => alert('x')} />;",
        "return <div {...props} />;",
        "return <>{props.label}</>;",
        "return <div>{props.disabled ? 'a' : 'b'}</div>;",
        "return <div>{items.map(x => <span>{x}</span>)}</div>;",
        "return <div>{props.missing}</div>;",
        "return <div>{props.disabled}</div>;",
        "return <div ref={props.label} />;",
        "return <div key=\"x\" />;",
        "return <div title=\"a\" title=\"b\" />;",
    ] {
        let source = format!(
            "type Props = {{ label: string; disabled: boolean; }}; export function Card(props: Props) {{ {body} }}"
        );
        let e = application::parse(&source, Language::Tsx).unwrap_err();
        assert_eq!(e.code, "unsupported", "{source}: {e:?}");
        assert!(e.span.end > e.span.start);
    }
    for source in [
        TSX.replace("label: string", "label?: string"),
        TSX.replace("label: string", "label: () => void"),
        format!("import './effect';\n{TSX}"),
        TSX.replace("function Card", "async function Card"),
        TSX.replace("props: Props", "{ label }: Props"),
    ] {
        assert_eq!(
            application::parse(&source, Language::Tsx).unwrap_err().code,
            "unsupported"
        );
    }
}

#[test]
fn unsupported_rust_never_executes_or_discards_code() {
    for source in [
        RUST.replace(
            "    element(\"section\"",
            "    println!(\"effect\"); element(\"section\"",
        ),
        RUST.replace(
            "text(props.label.clone())",
            "text(std::process::Command::new(\"x\").output())",
        ),
        RUST.replace("text(props.label.clone())", "text(props.missing)"),
        RUST.replace("pub label: String", "pub label: usize"),
        RUST.replace("pub fn Card", "pub async fn Card"),
        RUST.replace(
            "text(props.label.clone())",
            "#[cfg(any())] text(props.label.clone())",
        ),
        RUST.replace(
            "attribute(\"disabled\", props.disabled)",
            "attribute(\"onClick\", props.disabled)",
        ),
    ] {
        let e = application::parse(&source, Language::Rust).unwrap_err();
        assert_eq!(e.code, "unsupported", "{e:?}");
        assert!(e.span.end > e.span.start);
    }
}

#[test]
fn invalid_syntax_is_diagnostic_and_invalid_ir_cannot_be_emitted() {
    for language in [Language::Rust, Language::Tsx] {
        assert_eq!(
            application::parse("@@@", language).unwrap_err().code,
            "syntax"
        );
    }
    let mut ir = application::parse(TSX, Language::Tsx).unwrap();
    ir.name = "Injected;alert(1)".into();
    assert!(application::emit(&ir, Language::Tsx).is_err());
    ir.name = "Card".into();
    for keyword in ["type", "mut", "gen", "box", "interface", "arguments"] {
        ir.props[0].name = keyword.into();
        assert!(application::emit(&ir, Language::Rust).is_err());
    }
}

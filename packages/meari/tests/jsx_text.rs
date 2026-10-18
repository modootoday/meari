use meari::{
    application::Language,
    domain::{NodeKind, Value},
};

#[test]
fn jsx_literal_text_follows_pinned_typescript_whitespace_and_entity_order() {
    for (literal, expected) in [
        (
            "\n  First  line\n\n  second &amp; third\n",
            "First  line second & third",
        ),
        (" first\r\n\tsecond \r third ", " first second third "),
        (" \t ", " \t "),
        ("\n &nbsp;\n 한글 &#32;\n", "\u{a0} 한글  "),
        ("before\u{2028}after", "before after"),
        ("\n \t\n ", ""),
    ] {
        let source = format!(
            "type Props = {{}};export function TextDemo(props:Props){{return <div>{literal}</div>;}}"
        );
        let parsed = meari::parse(&source, Language::Tsx).unwrap();
        let NodeKind::Element { children, .. } = &parsed.root.kind else {
            panic!("element")
        };
        let actual = children
            .iter()
            .map(|node| match &node.kind {
                NodeKind::Text(Value::String(s)) => s.as_str(),
                _ => panic!("text"),
            })
            .collect::<String>();
        assert_eq!(actual, expected, "{literal:?}");
        for child in children {
            assert_eq!(&source[child.span.start..child.span.end], literal);
        }
        for language in [Language::Tsx, Language::Rust] {
            let emitted = meari::emit(&parsed, language).unwrap();
            assert!(parsed.semantic_eq(&meari::parse(&emitted.code, language).unwrap()));
        }
    }
}

#[test]
fn explicit_string_expressions_keep_whitespace_and_neighbor_boundaries() {
    let source = "type Props={};export function TextDemo(props:Props){return <div>\n  before\n  {\"  explicit\\ntext  \"}\n  after\n</div>;}";
    let parsed = meari::parse(source, Language::Tsx).unwrap();
    let NodeKind::Element { children, .. } = &parsed.root.kind else {
        panic!("element")
    };
    let actual: Vec<_> = children
        .iter()
        .map(|node| match &node.kind {
            NodeKind::Text(Value::String(s)) => s.as_str(),
            _ => panic!("text"),
        })
        .collect();
    assert_eq!(actual, vec!["before", "  explicit\ntext  ", "after"]);
}

#[test]
fn jsx_comment_containers_do_not_create_nodes_or_hide_executable_expressions() {
    let source = "type Props={};export function Comments(props:Props){return <div>{/* section */}<span>Content</span>{/* end */}</div>;}";
    let parsed = meari::parse(source, Language::Tsx).unwrap();
    let NodeKind::Element { children, .. } = &parsed.root.kind else {
        panic!("element")
    };
    assert_eq!(children.len(), 1);
    let emitted = meari::emit(&parsed, Language::Rust).unwrap();
    assert!(parsed.semantic_eq(&meari::parse(&emitted.code, Language::Rust).unwrap()));
    assert!(
        meari::parse(
            &source.replace("{/* section */}", "{sideEffect()}"),
            Language::Tsx
        )
        .is_err()
    );
}

use meari::{application::Language, emit, parse};

#[test]
fn client_entry_metadata_survives_both_frontends_and_original_spans() {
    let source = "\"use client\"; type Props={label:string}; export function Demo(props:Props){return <div>{props.label}</div>;}";
    let component = parse(source, Language::Tsx).unwrap();
    assert!(component.client);
    assert_eq!(
        &source[component.root.span.start..component.root.span.end],
        "<div>{props.label}</div>"
    );
    for language in [Language::Rust, Language::Tsx] {
        let output = emit(&component, language).unwrap();
        let parsed = parse(&output.code, language).unwrap();
        assert!(component.semantic_eq(&parsed));
    }
    let plain = parse(&source.replacen("\"use client\";", "", 1), Language::Tsx).unwrap();
    assert!(!component.semantic_eq(&plain));
}

#[test]
fn client_boundary_does_not_enable_server_directives_or_runtime_effects() {
    for source in [
        "\"use client\";type Props={};export function BRIDGE_USE_CLIENT(props:Props){return <div/>;}",
        "\"use server\"; type Props={};export function Demo(props:Props){return <div/>;}",
        "\"use client\";\"use client\";type Props={};export function Demo(props:Props){return <div/>;}",
        "\"use client\";type Props={};export function Demo(props:Props){const x=useState(0);return <div/>;}",
        "\"use client\";type Props={};export function Demo(props:Props){return <button onClick={() => alert('x')}/>;}",
    ] {
        assert!(parse(source, Language::Tsx).is_err());
    }
    let source = emit(
        &parse(
            "\"use client\";type Props={};export function Demo(props:Props){return <div/>;}",
            Language::Tsx,
        )
        .unwrap(),
        Language::Rust,
    )
    .unwrap()
    .code;
    assert!(parse(&source.replace("= true;", "= false;"), Language::Rust).is_err());
    assert!(
        parse(
            &("pub const BRIDGE_USE_CLIENT: bool = true;\n".to_owned() + &source),
            Language::Rust
        )
        .is_err()
    );
}

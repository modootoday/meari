use meari::{
    application::Language,
    domain::{Component, NodeKind, Value},
};

const SOURCE: &str = include_str!("../../fixtures/hooks/structured-updater.tsx");

fn roundtrip(component: &Component) {
    for language in [Language::Tsx, Language::Rust] {
        let generated = meari::emit(component, language).unwrap();
        let reparsed = meari::parse(&generated.code, language).unwrap();
        assert!(
            component.semantic_eq(&reparsed),
            "{language:?}: {}",
            generated.code
        );
    }
}

#[test]
fn real_functional_spread_and_captured_field_updates_roundtrip_both_frontends() {
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    roundtrip(&component);
    let captured = SOURCE.replace(
        "current => ({ ...current, count: current.count + 1, rows: state.rows })",
        "({ ...state, count: state.count + 1, rows: [state.rows[0]] })",
    );
    roundtrip(&meari::parse(&captured, Language::Tsx).unwrap());
    assert!(
        meari::application::state_program::StateProgram::mount(component, "root".into(), 1)
            .is_err()
    );
}

#[test]
fn generated_updater_parameter_never_shadows_a_free_state_named_previous() {
    let source = SOURCE
        .replace(
            "  return",
            "  const [previous, setPrevious] = useState(2);\n  return",
        )
        .replace("current.count + 1", "current.count + previous");
    let component = meari::parse(&source, Language::Tsx).unwrap();
    let code = meari::emit(&component, Language::Tsx).unwrap().code;
    assert!(code.contains("previous_ =>"), "{code}");
    roundtrip(&component);
}

#[test]
fn wrong_shapes_free_names_and_executable_or_dynamic_members_are_rejected() {
    let original = "current => ({ ...current, count: current.count + 1, rows: state.rows })";
    for bad in [
        "current => ({ ...current, count: true })",
        "current => ({ ...current, count: current.missing })",
        "current => ({ ...missing, count: 1 })",
        "current => ({ ...current, rows: [true] })",
        "current => ({ ...current, extra: 1 })",
        "current => ({ ...current, count: current.count + \"one\" })",
        "current => ({ ...current, count: current.rows[index] })",
        "current => ({ ...current, count: current?.count })",
        "current => ({ ...current, get count() { return 1; } })",
        "current => ({ ...current, count: sideEffect() })",
        "current => ({ ...current, __proto__: null })",
    ] {
        let error = meari::parse(&SOURCE.replace(original, bad), Language::Tsx).unwrap_err();
        assert_eq!(error.code, "unsupported", "{bad}");
        assert!(!error.message.contains("function(props"), "{error:?}");
    }
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    let NodeKind::Element { attributes, .. } = &component.root.kind else {
        panic!()
    };
    let Value::Event(event) = &attributes[0].value else {
        panic!()
    };
    assert!(
        event
            .capture(&Default::default())
            .unwrap_err()
            .contains("prepared hook document")
    );
}

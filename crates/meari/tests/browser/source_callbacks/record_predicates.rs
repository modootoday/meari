use super::*;
use meari::domain::{
    array_value::ArrayValue, host_callbacks::CallbackSignature, record_value::RecordValue,
};
const SOURCE: &str = include_str!("../../../fixtures/record-predicates/rail.tsx");

fn values(editable: bool) -> BTreeMap<String, Value> {
    let component = meari::parse(SOURCE, meari::application::Language::Tsx).unwrap();
    let PropType::Array(element) = &component.props[0].kind else {
        panic!("items")
    };
    let PropType::Record(schema) = element.as_ref() else {
        panic!("record")
    };
    BTreeMap::from([
        (
            "items".into(),
            Value::Array(ArrayValue {
                element: element.as_ref().clone(),
                items: vec![Value::Record(
                    RecordValue::new(
                        schema.clone(),
                        BTreeMap::from([
                            ("key".into(), Value::String("card".into())),
                            ("editable".into(), Value::Bool(editable)),
                        ]),
                    )
                    .unwrap(),
                )],
            }),
        ),
        ("selectedKey".into(), Value::String("card".into())),
        (
            "onSelect".into(),
            Value::CallbackReference(CallbackReference {
                binding: "onSelect".into(),
                signature: CallbackSignature::Text,
            }),
        ),
    ])
}

#[wasm_bindgen_test]
fn lexical_predicate_updates_browser_disabled_aria_and_callback_admission() {
    let entry: BuildPath = "rail.tsx".to_owned().try_into().unwrap();
    let graph =
        meari::parse_module_graph(&entry, &BTreeMap::from([(entry.clone(), SOURCE.into())]))
            .unwrap();
    let program =
        ModuleProgram::mount(graph, values(true), "browser-record-predicate".into(), 1).unwrap();
    let path = program
        .project()
        .unwrap()
        .callback_bindings()
        .keys()
        .next()
        .unwrap()
        .clone();
    let (mut document, host, container, _) = mount_program(program, "browser-record-predicate");
    let button = document
        .element(&path)
        .unwrap()
        .unchecked_into::<web_sys::HtmlButtonElement>();
    for editable in [true, false, true] {
        document
            .update_module_props_with_callbacks(
                values(editable),
                host.owner.export_bindings().unwrap(),
            )
            .unwrap();
        assert!(button.is_same_node(Some(document.element(&path).unwrap().as_ref())));
        assert_eq!(button.disabled(), !editable);
        assert_eq!(
            button.get_attribute("aria-disabled").as_deref(),
            Some(if editable { "false" } else { "true" })
        );
        button.click();
        let pending = document.drain_callbacks();
        assert_eq!(pending.len(), usize::from(editable));
        if editable {
            assert_eq!(
                pending[0].invocation.arguments,
                CallbackArguments::Text("card".into())
            );
        }
        assert!(document.last_error().is_none());
    }
    document.close();
    container.remove();
}

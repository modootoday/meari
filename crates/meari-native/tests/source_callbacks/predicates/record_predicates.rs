use super::*;
use meari::domain::{
    array_value::ArrayValue, host_callbacks::CallbackSignature, record_value::RecordValue,
};

const SOURCE: &str = include_str!("../../../../meari/fixtures/record-predicates/rail.tsx");

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

#[test]
fn lexical_disabled_updates_dom_a11y_and_callback_admission_on_same_node() {
    let initial = values(true);
    let (mut document, host) = mount_program(program(SOURCE, initial.clone()), initial);
    let path = document
        .module_projection()
        .unwrap()
        .callback_bindings()
        .keys()
        .next()
        .unwrap()
        .clone();
    let id = document.path_id(&path).unwrap().to_owned();
    let node_id = document.renderer.document.get_element_by_id(&id).unwrap();
    for editable in [true, false, true] {
        document
            .update_module_props_with_callbacks(
                values(editable),
                host.callbacks.export_bindings().unwrap(),
            )
            .unwrap();
        assert_eq!(
            document.renderer.document.get_element_by_id(&id),
            Some(node_id)
        );
        let node = document.renderer.document.get_node(node_id).unwrap();
        assert_eq!(
            node.attr(blitz_dom::LocalName::from("aria-disabled")),
            Some(if editable { "false" } else { "true" })
        );
        let tree = accessibility::tree(&document);
        let accessible = &tree
            .nodes
            .iter()
            .find(|(id, _)| id.0 == node_id.as_u64())
            .unwrap()
            .1;
        assert_eq!(accessible.is_disabled(), !editable);
        let dispatched = document.dispatch(Action::Activate { path: path.clone() });
        let callbacks = document.drain_callbacks();
        if editable {
            dispatched.unwrap();
            assert_eq!(callbacks.len(), 1);
            assert_eq!(
                callbacks[0].invocation.arguments,
                CallbackArguments::Text("card".into())
            );
        } else {
            assert!(callbacks.is_empty());
        }
    }
}

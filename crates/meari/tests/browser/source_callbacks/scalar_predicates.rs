use super::*;

#[wasm_bindgen_test]
fn original_queue_footer_state_predicates_retain_web_nodes_and_focus() {
    let source = include_str!("../../../fixtures/node-slots/studio/queue-footer.tsx");
    let (mut document, _host, container, _) = mount(source, "browser-state-predicate");
    let button = document
        .element("0/0/0")
        .unwrap()
        .dyn_into::<HtmlElement>()
        .unwrap();
    let section = document.element("0/1").unwrap();
    button.focus().unwrap();
    for expanded in [true, false, true] {
        button.click();
        assert!(
            document
                .element("0/0/0")
                .unwrap()
                .is_same_node(Some(button.as_ref()))
        );
        assert!(
            document
                .element("0/1")
                .unwrap()
                .is_same_node(Some(section.as_ref()))
        );
        assert_eq!(
            button.get_attribute("aria-expanded").as_deref(),
            Some(if expanded { "true" } else { "false" })
        );
        assert_eq!(section.has_attribute("hidden"), !expanded);
        assert!(
            button
                .owner_document()
                .unwrap()
                .active_element()
                .unwrap()
                .is_same_node(Some(button.as_ref()))
        );
    }
    document.close();
    container.remove();
}

const NAVIGATION: &str = include_str!("../../../fixtures/node-slots/studio/navigation.tsx");
const ITEM: &str = include_str!("../../../fixtures/node-slots/studio/navigation-item.tsx");

#[wasm_bindgen_test]
fn browser_original_navigation_comparison_preserves_nodes_and_updates_current_state() {
    let entry = BuildPath::try_from("studio/navigation.tsx".to_owned()).unwrap();
    let graph = meari::parse_module_graph(
        &entry,
        &BTreeMap::from([
            (entry.clone(), NAVIGATION.into()),
            (
                BuildPath::try_from("studio/navigation-item.tsx".to_owned()).unwrap(),
                ITEM.into(),
            ),
        ]),
    )
    .unwrap();
    let mut values = BTreeMap::from([
        ("selectedId".into(), Value::String("SEG".into())),
        (
            "onNavigate".into(),
            Value::CallbackReference(CallbackReference {
                binding: "onNavigate".into(),
                signature: meari::domain::host_callbacks::CallbackSignature::Text,
            }),
        ),
    ]);
    values.insert("selectedId".into(), Value::String("SEG".into()));
    let program = ModuleProgram::mount(
        graph,
        values.clone(),
        "browser-navigation-comparison".into(),
        1,
    )
    .unwrap();
    let handlers = program.project().unwrap().callback_bindings();
    assert_eq!(handlers.len(), 13);
    let (mut document, host, container, _) =
        mount_program(program, "browser-navigation-comparison");
    let buttons = handlers
        .iter()
        .map(|(path, handler)| {
            (
                path.clone(),
                document
                    .element(path)
                    .unwrap()
                    .unchecked_into::<HtmlElement>(),
                handler.captured_text.clone().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    for selected in ["SEG", "SUB", "NONE", "SEG"] {
        values.insert("selectedId".into(), Value::String(selected.into()));
        document
            .update_module_props_with_callbacks(
                values.clone(),
                host.owner.export_bindings().unwrap(),
            )
            .unwrap();
        for (path, button, captured) in &buttons {
            assert!(button.is_same_node(Some(document.element(path).unwrap().as_ref())));
            assert_eq!(
                button.get_attribute("aria-current").as_deref(),
                Some(if captured == selected {
                    "true"
                } else {
                    "false"
                })
            );
            button.click();
            assert!(
                document.last_error().is_none(),
                "{:?}",
                document.last_error()
            );
            let pending = document.drain_callbacks();
            assert_eq!(pending.len(), 1);
            assert_eq!(
                pending[0].invocation.arguments,
                CallbackArguments::Text(captured.clone())
            );
        }
    }
    assert!(host.requests.borrow().is_empty());
    document.close();
    container.remove();
}

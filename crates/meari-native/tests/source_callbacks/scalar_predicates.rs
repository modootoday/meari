use super::*;

const NAVIGATION: &str = include_str!("../../../meari/fixtures/node-slots/studio/navigation.tsx");
const ITEM: &str = include_str!("../../../meari/fixtures/node-slots/studio/navigation-item.tsx");

#[test]
fn original_navigation_comparison_updates_selected_button_without_changing_capture_identity() {
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
    let program =
        ModuleProgram::mount(graph, values.clone(), "navigation-comparison".into(), 1).unwrap();
    let (mut document, mut host) = mount_program(program, values.clone());
    let button_paths = document
        .module_projection()
        .unwrap()
        .callback_bindings()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(button_paths.len(), 13);
    let initial_nodes = button_paths
        .iter()
        .map(|path| {
            (
                path.clone(),
                document
                    .renderer
                    .document
                    .get_element_by_id(document.path_id(path).unwrap())
                    .unwrap(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for selected in ["SEG", "SUB", "NONE", "SEG"] {
        values.insert("selectedId".into(), Value::String(selected.into()));
        document
            .update_module_props_with_callbacks(
                values.clone(),
                host.callbacks.export_bindings().unwrap(),
            )
            .unwrap();
        let projection = document.module_projection().unwrap();
        let callbacks = projection.callback_bindings();
        let tree = accessibility::tree(&document);
        assert_eq!(callbacks.keys().cloned().collect::<Vec<_>>(), button_paths);
        let mut selected_count = 0;
        for (path, callback) in callbacks {
            let button = document
                .elements()
                .iter()
                .find(|element| element.path == path)
                .unwrap();
            let current = button.attributes.get("aria-current").map(String::as_str) == Some("true");
            let expected = callback.captured_text.as_deref() == Some(selected);
            assert_eq!(current, expected);
            let node = document
                .renderer
                .document
                .get_element_by_id(&button.id)
                .unwrap();
            assert_eq!(node, initial_nodes[&path]);
            assert_eq!(
                document
                    .renderer
                    .document
                    .get_node(node)
                    .unwrap()
                    .attr(blitz_dom::LocalName::from("aria-current")),
                Some(if expected { "true" } else { "false" })
            );
            let accessible = &tree
                .nodes
                .iter()
                .find(|(id, _)| id.0 == node.as_u64())
                .unwrap()
                .1;
            assert_eq!(
                accessible.aria_current(),
                if expected {
                    Some(accesskit::AriaCurrent::True)
                } else {
                    None
                }
            );
            selected_count += usize::from(current);
            document.dispatch(Action::Activate { path }).unwrap();
            let pending = document.drain_callbacks();
            assert_eq!(pending.len(), 1);
            assert_eq!(
                pending[0].invocation.arguments,
                CallbackArguments::Text(callback.captured_text.unwrap())
            );
        }
        assert_eq!(selected_count, usize::from(selected != "NONE"));
    }
    assert!(host.received.borrow().is_empty());
    let before = document.module_projection().unwrap().component.clone();
    values.insert("selectedId".into(), Value::Bool(false));
    assert!(
        document
            .update_module_props_with_callbacks(values, host.callbacks.export_bindings().unwrap())
            .is_err()
    );
    assert!(
        document
            .module_projection()
            .unwrap()
            .component
            .semantic_eq(&before)
    );
    // The same host registry remains usable after a failed props candidate.
    document
        .dispatch(Action::Activate {
            path: button_paths[0].clone(),
        })
        .unwrap();
    let pending = document.drain_callbacks().pop().unwrap();
    host.callbacks
        .dispatch(
            &pending.handle,
            pending.invocation.arguments,
            &mut host.registry,
        )
        .unwrap();
    assert_eq!(host.received.borrow().len(), 1);
}

#[test]
fn native_current_tokens_follow_wai_aria_without_turning_false_into_true() {
    use accesskit::AriaCurrent as Current;
    for (token, expected) in [
        (None, None),
        (Some(""), None),
        (Some("undefined"), None),
        (Some("false"), None),
        (Some("FALSE"), None),
        (Some("true"), Some(Current::True)),
        (Some("page"), Some(Current::Page)),
        (Some("Page"), Some(Current::Page)),
        (Some("step"), Some(Current::Step)),
        (Some("location"), Some(Current::Location)),
        (Some("date"), Some(Current::Date)),
        (Some("time"), Some(Current::Time)),
        (Some("unknown"), Some(Current::True)),
        (Some("false "), Some(Current::True)),
        (Some(" "), Some(Current::True)),
    ] {
        let attribute = if token.is_some() {
            "aria-current={props.current}"
        } else {
            ""
        };
        let source = format!(
            "type Props={{current:string}};export function Current(props:Props){{return <Button aria-label=\"current token\" {attribute}/>;}}"
        );
        let mut values = props(&source);
        values.insert("current".into(), Value::String(token.unwrap_or("").into()));
        let (document, _host) = mount_program(program(&source, values.clone()), values);
        let button = document
            .elements()
            .iter()
            .find(|element| {
                element.attributes.get("aria-label").map(String::as_str) == Some("current token")
            })
            .unwrap();
        let node = document
            .renderer
            .document
            .get_element_by_id(&button.id)
            .unwrap();
        let tree = accessibility::tree(&document);
        let accessible = &tree
            .nodes
            .iter()
            .find(|(id, _)| id.0 == node.as_u64())
            .unwrap()
            .1;
        assert_eq!(accessible.aria_current(), expected, "{token:?}");
        assert_eq!(accessible.role(), accesskit::Role::Button);
        assert_eq!(accessible.label(), Some("current token"));
    }
}

#[test]
fn current_property_clears_after_false_or_empty_updates_on_the_same_button() {
    let source = "type Props={current:string};export function Current(props:Props){return <Button aria-label=\"current transition\" aria-current={props.current}/>;}";
    let mut values = props(source);
    values.insert("current".into(), Value::String("page".into()));
    let (mut document, host) = mount_program(program(source, values.clone()), values.clone());
    let button = document
        .elements()
        .iter()
        .find(|element| {
            element.attributes.get("aria-label").map(String::as_str) == Some("current transition")
        })
        .unwrap()
        .clone();
    let original = document
        .renderer
        .document
        .get_element_by_id(&button.id)
        .unwrap();
    for (token, expected) in [
        ("page", Some(accesskit::AriaCurrent::Page)),
        ("false", None),
        ("", None),
        ("step", Some(accesskit::AriaCurrent::Step)),
        ("undefined", None),
        ("false", None),
    ] {
        values.insert("current".into(), Value::String(token.into()));
        document
            .update_module_props_with_callbacks(
                values.clone(),
                host.callbacks.export_bindings().unwrap(),
            )
            .unwrap();
        assert_eq!(
            document.renderer.document.get_element_by_id(&button.id),
            Some(original)
        );
        assert_eq!(
            document
                .renderer
                .document
                .get_node(original)
                .unwrap()
                .attr(blitz_dom::LocalName::from("aria-current")),
            Some(token)
        );
        let tree = accessibility::tree(&document);
        assert_eq!(
            tree.nodes
                .iter()
                .find(|(id, _)| id.0 == original.as_u64())
                .unwrap()
                .1
                .aria_current(),
            expected
        );
    }
}

use meari::{
    application::{host_callbacks::HostCallbacks, plugins::*, source_modules::ModuleProgram},
    domain::{
        PropType, Value,
        build::{BuildPath, ContentHash, TargetKind},
        host_callbacks::{CallbackArguments, CallbackReference},
    },
};
#[path = "range_commit/mod.rs"]
mod range_commit;
use meari_native::{
    domain::{Viewport, widgets::Action},
    infrastructure::{accessibility, document::InteractiveDocument},
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
#[path = "captured_text.rs"]
mod captured_text;
#[path = "host_runtime.rs"]
mod host_runtime;
#[path = "mixed_migration.rs"]
mod mixed_migration;
#[path = "mixed_slots.rs"]
mod mixed_slots;
#[path = "radio_group.rs"]
mod radio_group;
#[path = "predicates/record_predicates.rs"]
mod record_predicates;
#[path = "predicates/scalar_predicates.rs"]
mod scalar_predicates;

const REVIEW: &str = include_str!("../../../meari/fixtures/host-callback-review.tsx");
const CONVERT: &str = include_str!("../../../meari/fixtures/host-callback-convert.tsx");
const ACTIONS: &str =
    include_str!("../../../meari/fixtures/array-callback-graph/components/proposal-actions.tsx");

#[test]
fn original_card_and_highlight_graphs_mount_both_refs_and_live_action_callbacks() {
    use meari::runtime::IntoValue;
    for (entry, source) in [
        (
            "studio/cards/proposal-panel.tsx",
            include_str!(
                "../../../meari/fixtures/array-callback-graph/studio/cards/proposal-panel.tsx"
            ),
        ),
        (
            "studio/clips/highlight-proposal-panel.tsx",
            include_str!(
                "../../../meari/fixtures/array-callback-graph/studio/clips/highlight-proposal-panel.tsx"
            ),
        ),
    ] {
        let entry: BuildPath = entry.to_owned().try_into().unwrap();
        let graph = meari::parse_module_graph(
            &entry,
            &BTreeMap::from([
                (entry.clone(), source.into()),
                (
                    "components/proposal-source.tsx"
                        .to_owned()
                        .try_into()
                        .unwrap(),
                    include_str!(
                        "../../../meari/fixtures/array-callback-graph/components/proposal-source.tsx"
                    )
                    .into(),
                ),
                (
                    "components/proposal-actions.tsx"
                        .to_owned()
                        .try_into()
                        .unwrap(),
                    ACTIONS.into(),
                ),
            ]),
        )
        .unwrap();
        let values = graph.modules[&entry]
            .component
            .props
            .iter()
            .map(|prop| {
                let value = match &prop.kind {
                    PropType::String => Value::String(format!("原文🦀:{}", prop.name)),
                    PropType::Bool => Value::Bool(prop.name.starts_with("can")),
                    PropType::Array(element)
                        if element.as_ref() == &PropType::String && prop.name == "sourceRefs" =>
                    {
                        vec!["cue-a", "cue-b"].into_value()
                    }
                    PropType::Callback(signature) => Value::CallbackReference(CallbackReference {
                        binding: prop.name.clone(),
                        signature: *signature,
                    }),
                    _ => panic!("original fixture requires explicit binding: {}", prop.name),
                };
                (prop.name.clone(), value)
            })
            .collect::<BTreeMap<_, _>>();
        let program =
            ModuleProgram::mount(graph, values.clone(), "original-repeat".into(), 1).unwrap();
        let (mut document, _host) = mount_program(program, values);
        for reference in ["cue-a", "cue-b"] {
            assert_eq!(
                document
                    .elements()
                    .iter()
                    .flat_map(|element| &element.text_runs)
                    .filter(|run| run.text == reference)
                    .count(),
                1
            );
        }
        let save = document
            .module_projection()
            .unwrap()
            .callback_bindings()
            .into_iter()
            .find(|(_, handler)| handler.reference.binding == "onSave")
            .unwrap()
            .0;
        document.dispatch(Action::Activate { path: save }).unwrap();
        assert_eq!(document.drain_callbacks().len(), 1);
        let summary = document
            .elements()
            .iter()
            .find(|element| {
                element
                    .text_runs
                    .iter()
                    .any(|run| run.text == "생성 원문 확인")
            })
            .unwrap();
        let summary_node = document
            .renderer
            .document
            .get_element_by_id(&summary.id)
            .unwrap();
        let details = document
            .renderer
            .document
            .get_node(summary_node)
            .unwrap()
            .parent
            .unwrap();
        let tree = accessibility::tree(&document);
        let (_, summary_accessible) = tree
            .nodes
            .iter()
            .find(|(id, _)| id.0 == summary_node.as_u64())
            .unwrap();
        assert_eq!(
            summary_accessible.role(),
            accesskit::Role::DisclosureTriangle
        );
        assert!(summary_accessible.supports_action(accesskit::Action::Expand));
        for action in [accesskit::Action::Expand, accesskit::Action::Collapse] {
            accessibility::act(
                &mut document,
                &accesskit::ActionRequest {
                    action,
                    target_tree: accesskit::TreeId::ROOT,
                    target_node: accesskit::NodeId(summary_node.as_u64()),
                    data: None,
                },
            )
            .unwrap();
            assert_eq!(
                document
                    .renderer
                    .document
                    .get_node(details)
                    .unwrap()
                    .attr(blitz_dom::LocalName::from("open"))
                    .is_some(),
                action == accesskit::Action::Expand
            );
        }
    }
}
fn hash(value: &str) -> ContentHash {
    ContentHash::of(value.as_bytes())
}
struct Recorder {
    manifest: PluginManifest,
    received: Rc<RefCell<Vec<serde_json::Value>>>,
}
impl PluginAdapter for Recorder {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }
    fn validate_payload(
        &self,
        _: &str,
        _: PayloadDirection,
        _: &PluginPayload,
    ) -> Result<(), PluginError> {
        Ok(())
    }
    fn dispatch(&mut self, invocation: &PluginInvocation) -> Result<PluginDispatch, PluginError> {
        self.received
            .borrow_mut()
            .push(invocation.payload.value.clone());
        Ok(PluginDispatch::Immediate(Ok(PluginPayload {
            schema: hash("reply"),
            value: serde_json::Value::Null,
        })))
    }
    fn cancel(&mut self, _: &PluginTicket) -> Result<CancelReceipt, PluginError> {
        Ok(CancelReceipt::AlreadyTerminal)
    }
    fn shutdown(&mut self) -> Result<(), PluginError> {
        Ok(())
    }
}
fn props(source: &str) -> BTreeMap<String, Value> {
    meari::parse(source, meari::application::Language::Tsx)
        .unwrap()
        .props
        .into_iter()
        .map(|prop| {
            let value = match prop.kind {
                PropType::String => Value::String(
                    if prop.name == "workingText" {
                        "원문🦀"
                    } else {
                        "value"
                    }
                    .into(),
                ),
                PropType::Bool => Value::Bool(prop.name == "canConvert"),
                PropType::Callback(signature) => Value::CallbackReference(CallbackReference {
                    binding: prop.name.clone(),
                    signature,
                }),
                PropType::Array(_) => panic!("callback fixture requires explicit array values"),
                PropType::Node => panic!("callback fixture requires explicit node slots"),
                PropType::Record(_) => panic!("callback fixture requires explicit record values"),
            };
            (prop.name, value)
        })
        .collect()
}
fn program(source: &str, values: BTreeMap<String, Value>) -> ModuleProgram {
    let entry: BuildPath = "view.tsx".to_owned().try_into().unwrap();
    let graph =
        meari::parse_module_graph(&entry, &BTreeMap::from([(entry.clone(), source.into())]))
            .unwrap();
    ModuleProgram::mount(graph, values, "callback-document".into(), 1).unwrap()
}
struct Host {
    callbacks: HostCallbacks,
    registry: PluginRegistry,
    received: Rc<RefCell<Vec<serde_json::Value>>>,
}
fn mount(source: &str) -> (InteractiveDocument, Host) {
    let values = props(source);
    let program = program(source, values.clone());
    mount_program(program, values)
}
fn mount_program(
    program: ModuleProgram,
    values: BTreeMap<String, Value>,
) -> (InteractiveDocument, Host) {
    let names: Vec<_> = values
        .values()
        .filter_map(|value| match value {
            Value::CallbackReference(reference) => Some(reference.clone()),
            _ => None,
        })
        .collect();
    let mut host = None;
    let document = InteractiveDocument::from_module_program_with_callbacks(
        program,
        "button{width:150px;height:35px}textarea{width:250px;height:70px}",
        Viewport {
            width: 640,
            height: 480,
            dark: false,
        },
        None,
        7,
        |scope| {
            let received = Rc::new(RefCell::new(Vec::new()));
            let mut registry =
                PluginRegistry::new(hash("registry"), TargetKind::Native, scope.clone(), 8)
                    .unwrap();
            let manifest = PluginManifest {
                api_version: PLUGIN_API_VERSION,
                id: "test.intents".into(),
                version: "1".into(),
                contract: hash("contract"),
                provider: hash("provider"),
                assets: BTreeMap::new(),
                capabilities: BTreeMap::from([(
                    "intent".into(),
                    CapabilityDescriptor {
                        targets: BTreeSet::from([TargetKind::Native]),
                        request_schema: hash("request"),
                        reply_schema: hash("reply"),
                        error_schema: None,
                        event_schema: None,
                    },
                )]),
            };
            let registration = registry
                .register(
                    Box::new(Recorder {
                        manifest,
                        received: received.clone(),
                    }),
                    BTreeSet::from(["intent".into()]),
                )
                .unwrap();
            let mut callbacks = HostCallbacks::new(scope);
            for reference in names {
                let name = reference.binding.clone();
                callbacks
                    .bind(
                        reference.binding,
                        reference.signature,
                        registration.clone(),
                        "intent".into(),
                        move |arguments| {
                            Ok(PluginPayload {
                                schema: hash("request"),
                                value: serde_json::json!({"name": name, "arguments": arguments}),
                            })
                        },
                    )
                    .unwrap();
            }
            let export = callbacks.export_bindings().unwrap();
            host = Some(Host {
                callbacks,
                registry,
                received,
            });
            Ok(export)
        },
    )
    .unwrap();
    (document, host.unwrap())
}
fn editor(document: &InteractiveDocument) -> (String, blitz_dom::NodeId) {
    let element = document
        .elements()
        .iter()
        .find(|element| element.attributes.contains_key("value"))
        .unwrap();
    (
        element.path.clone(),
        document
            .renderer
            .document
            .get_element_by_id(&element.id)
            .unwrap(),
    )
}
fn text(document: &InteractiveDocument, node: blitz_dom::NodeId) -> String {
    document
        .renderer
        .document
        .get_node(node)
        .unwrap()
        .element_data()
        .unwrap()
        .text_input_data()
        .unwrap()
        .editor
        .raw_text()
        .to_string()
}

#[test]
fn original_review_document_defers_typed_text_and_actions_to_one_registry_and_preserves_editor() {
    let (mut document, mut host) = mount(REVIEW);
    let (path, node) = editor(&document);
    assert_eq!(text(&document, node), "원문🦀");
    document.set_input_value(&path, "수정🦀").unwrap();
    let save = document
        .module_projection()
        .unwrap()
        .callback_bindings()
        .into_iter()
        .find(|(_, handler)| handler.reference.binding == "onSave")
        .unwrap()
        .0;
    document.dispatch(Action::Activate { path: save }).unwrap();
    assert!(host.received.borrow().is_empty());
    let pending = document.drain_callbacks();
    assert_eq!(pending.len(), 2);
    assert_eq!(
        pending[0].invocation.arguments,
        CallbackArguments::Text("수정🦀".into())
    );
    for item in pending {
        host.callbacks
            .dispatch(&item.handle, item.invocation.arguments, &mut host.registry)
            .unwrap();
    }
    assert_eq!(host.received.borrow().len(), 2);
    let mut values = props(REVIEW);
    values.insert("workingText".into(), Value::String("수정🦀".into()));
    document
        .update_module_props_with_callbacks(values, host.callbacks.export_bindings().unwrap())
        .unwrap();
    assert_eq!(editor(&document).1, node);
    assert_eq!(text(&document, node), "수정🦀");
    let tree = accessibility::tree(&document);
    let entry = &tree
        .nodes
        .iter()
        .find(|(id, _)| id.0 == node.as_u64())
        .unwrap()
        .1;
    assert_eq!(entry.label(), Some("선택한 필드의 수정 문구"));
    assert_eq!(entry.value(), Some("수정🦀"));
}

#[test]
fn original_convert_disabled_and_export_replacement_keep_last_good_and_expire_drained_requests() {
    let (mut document, mut host) = mount(CONVERT);
    let path = document
        .module_projection()
        .unwrap()
        .callback_bindings()
        .keys()
        .next()
        .unwrap()
        .clone();
    document
        .dispatch(Action::Activate { path: path.clone() })
        .unwrap();
    let pending = document.drain_callbacks().pop().unwrap();
    let mut invalid = props(CONVERT);
    invalid.remove("canConvert");
    assert!(
        document
            .update_module_props_with_callbacks(invalid, host.callbacks.export_bindings().unwrap())
            .is_err()
    );
    host.callbacks
        .dispatch(
            &pending.handle,
            CallbackArguments::Action,
            &mut host.registry,
        )
        .unwrap();
    document
        .dispatch(Action::Activate { path: path.clone() })
        .unwrap();
    let stale = document.drain_callbacks().pop().unwrap();
    let mut disabled = props(CONVERT);
    disabled.insert("canConvert".into(), Value::Bool(false));
    document
        .update_module_props_with_callbacks(disabled, host.callbacks.export_bindings().unwrap())
        .unwrap();
    assert_eq!(
        host.callbacks
            .dispatch(&stale.handle, CallbackArguments::Action, &mut host.registry)
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    assert!(document.dispatch(Action::Activate { path }).is_err());
    assert!(document.drain_callbacks().is_empty());
    assert_eq!(host.received.borrow().len(), 1);
}

#[test]
fn text_guards_queue_budget_and_unmount_revoke_handles() {
    let (mut document, mut host) = mount(REVIEW);
    let (path, _) = editor(&document);
    assert!(
        document
            .enqueue_callback_text(&path, &"x".repeat(1024 * 1024 + 1))
            .is_err()
    );
    document.enqueue_callback_text(&path, "pending").unwrap();
    let pending = document.drain_callbacks().pop().unwrap();
    document
        .replace_stylesheet("textarea{visibility:hidden}")
        .unwrap();
    assert!(document.enqueue_callback_text(&path, "hidden").is_err());
    drop(document);
    assert_eq!(
        host.callbacks
            .dispatch(
                &pending.handle,
                pending.invocation.arguments,
                &mut host.registry
            )
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    assert!(host.received.borrow().is_empty());
}

#[test]
fn original_actions_composite_predicates_guard_the_actual_native_callback_queue() {
    let (mut document, host) = mount(ACTIONS);
    let path = document
        .module_projection()
        .unwrap()
        .callback_bindings()
        .into_iter()
        .find(|(_, handler)| handler.reference.binding == "onSave")
        .unwrap()
        .0;
    for can_save in [false, true] {
        for busy in [false, true] {
            for refresh in [false, true] {
                let mut values = props(ACTIONS);
                values.insert("canSave".into(), Value::Bool(can_save));
                values.insert("busy".into(), Value::Bool(busy));
                values.insert("refreshRequired".into(), Value::Bool(refresh));
                document
                    .update_module_props_with_callbacks(
                        values,
                        host.callbacks.export_bindings().unwrap(),
                    )
                    .unwrap();
                let disabled = !can_save || busy || refresh;
                let result = document.dispatch(Action::Activate { path: path.clone() });
                assert_eq!(result.is_err(), disabled);
                assert_eq!(document.drain_callbacks().len(), usize::from(!disabled));
            }
        }
    }
    assert!(host.received.borrow().is_empty());
}

#[test]
fn actual_engine_keyboard_and_accesskit_emit_once_without_registry_reentry() {
    use blitz_dom::Document;
    use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
    use keyboard_types::{Code, Key, Location, Modifiers};
    let (mut document, host) = mount(REVIEW);
    let (path, node) = editor(&document);
    document.focus_path(&path).unwrap();
    document.handle_ui_event(UiEvent::KeyDown(BlitzKeyEvent {
        key: Key::Character("a".into()),
        code: Code::KeyA,
        modifiers: Modifiers::empty(),
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state: KeyState::Pressed,
        text: Some("a".into()),
    }));
    assert!(document.last_error.is_none(), "{:?}", document.last_error);
    let pending = document.drain_callbacks();
    assert_eq!(pending.len(), 1);
    assert!(
        matches!(&pending[0].invocation.arguments, CallbackArguments::Text(value) if value.contains('a'))
    );
    accessibility::act(
        &mut document,
        &accesskit::ActionRequest {
            action: accesskit::Action::SetValue,
            target_tree: accesskit::TreeId::ROOT,
            target_node: accesskit::NodeId(node.as_u64()),
            data: Some(accesskit::ActionData::Value("accesskit text".into())),
        },
    )
    .unwrap();
    assert_eq!(
        document.drain_callbacks()[0].invocation.arguments,
        CallbackArguments::Text("accesskit text".into())
    );
    let save = document
        .module_projection()
        .unwrap()
        .callback_bindings()
        .into_iter()
        .find(|(_, handler)| handler.reference.binding == "onSave")
        .unwrap()
        .0;
    let button = document
        .renderer
        .document
        .get_element_by_id(document.path_id(&save).unwrap())
        .unwrap();
    accessibility::act(
        &mut document,
        &accesskit::ActionRequest {
            action: accesskit::Action::Click,
            target_tree: accesskit::TreeId::ROOT,
            target_node: accesskit::NodeId(button.as_u64()),
            data: None,
        },
    )
    .unwrap();
    assert_eq!(document.drain_callbacks().len(), 1);
    document.focus_path(&save).unwrap();
    document.handle_ui_event(UiEvent::KeyDown(BlitzKeyEvent {
        key: Key::Named(keyboard_types::NamedKey::Enter),
        code: Code::Enter,
        modifiers: Modifiers::empty(),
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state: KeyState::Pressed,
        text: None,
    }));
    assert_eq!(document.drain_callbacks().len(), 1);
    assert!(host.received.borrow().is_empty());
}

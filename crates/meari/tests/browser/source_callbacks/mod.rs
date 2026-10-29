use meari::{
    application::{host_callbacks::HostCallbacks, plugins::*, source_modules::ModuleProgram},
    domain::{
        PropType, Value,
        build::{BuildPath, ContentHash, TargetKind},
        host_callbacks::{CallbackArguments, CallbackReference},
    },
    infrastructure::web_document::WebDocument,
};
#[path = "range_commit/mod.rs"]
mod range_commit;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{Element, Event, HtmlElement, HtmlTextAreaElement};
const REVIEW: &str = include_str!("../../../fixtures/host-callback-review.tsx");
const CONVERT: &str = include_str!("../../../fixtures/host-callback-convert.tsx");
#[path = "captured_text.rs"]
mod captured_text;
#[path = "mixed_migration.rs"]
mod mixed_migration;
#[path = "radio_group.rs"]
mod radio_group;
#[path = "record_predicates.rs"]
mod record_predicates;
#[path = "scalar_predicates.rs"]
mod scalar_predicates;
fn hash(value: &str) -> ContentHash {
    ContentHash::of(value.as_bytes())
}
struct Recorder {
    manifest: PluginManifest,
    requests: Rc<RefCell<Vec<serde_json::Value>>>,
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
    fn dispatch(&mut self, request: &PluginInvocation) -> Result<PluginDispatch, PluginError> {
        self.requests
            .borrow_mut()
            .push(request.payload.value.clone());
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
struct Host {
    owner: HostCallbacks,
    registry: PluginRegistry,
    requests: Rc<RefCell<Vec<serde_json::Value>>>,
}
type Mounted = (WebDocument, Host, Element, BTreeMap<String, String>);
fn mount(source: &str, name: &str) -> Mounted {
    let entry: BuildPath = "view.tsx".to_owned().try_into().unwrap();
    let graph =
        meari::parse_module_graph(&entry, &BTreeMap::from([(entry.clone(), source.into())]))
            .unwrap();
    let program = ModuleProgram::mount(graph, props(source), name.into(), 1).unwrap();
    mount_program(program, name)
}
fn mount_program(program: ModuleProgram, name: &str) -> Mounted {
    let handlers = program.project().unwrap().callback_bindings();
    let paths = handlers
        .iter()
        .map(|(path, handler)| (handler.reference.binding.clone(), path.clone()))
        .collect();
    let container = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .create_element("section")
        .unwrap();
    web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .body()
        .unwrap()
        .append_child(&container)
        .unwrap();
    let mut host = None;
    let document = WebDocument::mount_module_program_with_callbacks(
        &container,
        program,
        "button{width:150px;height:35px}textarea{width:250px;height:70px}",
        (name.into(), 1),
        7,
        |scope| {
            let requests = Rc::new(RefCell::new(Vec::new()));
            let mut registry =
                PluginRegistry::new(hash(name), TargetKind::Web, scope.clone(), 8).unwrap();
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
                        targets: BTreeSet::from([TargetKind::Web]),
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
                        requests: requests.clone(),
                    }),
                    BTreeSet::from(["intent".into()]),
                )
                .unwrap();
            let mut owner = HostCallbacks::new(scope);
            let mut names = BTreeSet::new();
            for handler in handlers.values() {
                let reference = handler.reference.clone();
                if !names.insert(reference.binding.clone()) {
                    continue;
                }
                let name = reference.binding.clone();
                owner
                    .bind(
                        reference.binding,
                        reference.signature,
                        registration.clone(),
                        "intent".into(),
                        move |arguments| {
                            Ok(PluginPayload {
                                schema: hash("request"),
                                value: serde_json::json!({"name":name,"arguments":arguments}),
                            })
                        },
                    )
                    .unwrap();
            }
            let export = owner.export_bindings().unwrap();
            host = Some(Host {
                owner,
                registry,
                requests,
            });
            Ok(export)
        },
    )
    .unwrap();
    (document, host.unwrap(), container, paths)
}
#[wasm_bindgen_test]
fn browser_original_review_callbacks_preserve_text_identity_and_defer_registry_until_drain() {
    let (mut document, mut host, container, paths) = mount(REVIEW, "browser-review-callbacks");
    let input: HtmlTextAreaElement = document
        .element(&paths["onDraftChange"])
        .unwrap()
        .unchecked_into();
    assert_eq!(input.value(), "원문🦀");
    input.set_value("수정🦀");
    input.dispatch_event(&Event::new("input").unwrap()).unwrap();
    let button: HtmlElement = document.element(&paths["onSave"]).unwrap().unchecked_into();
    button.click();
    assert!(
        document.last_error().is_none(),
        "{:?}",
        document.last_error()
    );
    assert!(host.requests.borrow().is_empty());
    let pending = document.drain_callbacks();
    assert_eq!(pending.len(), 2);
    assert_eq!(
        pending[0].invocation.arguments,
        CallbackArguments::Text("수정🦀".into())
    );
    for request in pending {
        host.owner
            .dispatch(
                &request.handle,
                request.invocation.arguments,
                &mut host.registry,
            )
            .unwrap();
    }
    assert_eq!(host.requests.borrow().len(), 2);
    let mut values = props(REVIEW);
    values.insert("workingText".into(), Value::String("saved🦀".into()));
    document
        .update_module_props_with_callbacks(values, host.owner.export_bindings().unwrap())
        .unwrap();
    assert_eq!(input.value(), "saved🦀");
    assert!(input.is_same_node(Some(
        document.element(&paths["onDraftChange"]).unwrap().as_ref()
    )));
    input.set_value("later");
    input.dispatch_event(&Event::new("input").unwrap()).unwrap();
    let stale = document.drain_callbacks().pop().unwrap();
    document.close();
    assert_eq!(
        host.owner
            .dispatch(
                &stale.handle,
                stale.invocation.arguments,
                &mut host.registry
            )
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    button.click();
    assert!(document.drain_callbacks().is_empty());
    container.remove();
}
#[wasm_bindgen_test]
fn browser_original_convert_atomic_export_update_preserves_failed_last_good_and_disabled_guard() {
    let (mut document, mut host, container, paths) = mount(CONVERT, "browser-convert-callbacks");
    let button: HtmlElement = document
        .element(&paths["onConvert"])
        .unwrap()
        .unchecked_into();
    button.click();
    let pending = document.drain_callbacks().pop().unwrap();
    let mut invalid = props(CONVERT);
    invalid.remove("canConvert");
    assert!(
        document
            .update_module_props_with_callbacks(invalid, host.owner.export_bindings().unwrap())
            .is_err()
    );
    host.owner
        .dispatch(
            &pending.handle,
            pending.invocation.arguments,
            &mut host.registry,
        )
        .unwrap();
    button.click();
    let stale = document.drain_callbacks().pop().unwrap();
    let mut values = props(CONVERT);
    values.insert("canConvert".into(), Value::Bool(false));
    document
        .update_module_props_with_callbacks(values, host.owner.export_bindings().unwrap())
        .unwrap();
    assert_eq!(
        host.owner
            .dispatch(
                &stale.handle,
                stale.invocation.arguments,
                &mut host.registry
            )
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    button
        .dispatch_event(&Event::new("click").unwrap())
        .unwrap();
    assert!(document.last_error().is_some());
    assert!(document.drain_callbacks().is_empty());
    assert_eq!(host.requests.borrow().len(), 1);
    drop(document);
    container.remove();
}

#[wasm_bindgen_test]
fn browser_repeat_original_graphs_keep_all_refs_disclosure_and_action_callbacks() {
    use meari::runtime::IntoValue;
    for (entry, source) in [
        (
            "studio/cards/proposal-panel.tsx",
            include_str!("../../../fixtures/array-callback-graph/studio/cards/proposal-panel.tsx"),
        ),
        (
            "studio/clips/highlight-proposal-panel.tsx",
            include_str!(
                "../../../fixtures/array-callback-graph/studio/clips/highlight-proposal-panel.tsx"
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
                        "../../../fixtures/array-callback-graph/components/proposal-source.tsx"
                    )
                    .into(),
                ),
                (
                    "components/proposal-actions.tsx"
                        .to_owned()
                        .try_into()
                        .unwrap(),
                    include_str!(
                        "../../../fixtures/array-callback-graph/components/proposal-actions.tsx"
                    )
                    .into(),
                ),
            ]),
        )
        .unwrap();
        let mut values = graph.modules[&entry]
            .component
            .props
            .iter()
            .map(|prop| {
                let value = match &prop.kind {
                    PropType::String => Value::String(format!("원문🦀:{}", prop.name)),
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
            ModuleProgram::mount(graph, values.clone(), "web-repeat-original".into(), 1).unwrap();
        let (mut document, host, container, paths) = mount_program(program, "web-repeat-original");
        let rows = container.query_selector_all("[data-meari-repeat]").unwrap();
        assert_eq!(rows.length(), 2);
        assert_eq!(
            rows.item(0).unwrap().text_content().as_deref(),
            Some("cue-a")
        );
        assert_eq!(
            rows.item(1).unwrap().text_content().as_deref(),
            Some("cue-b")
        );
        let retained = rows.item(0).unwrap();
        let details = container.query_selector("details").unwrap().unwrap();
        let summary: HtmlElement = container
            .query_selector("summary")
            .unwrap()
            .unwrap()
            .unchecked_into();
        assert!(!details.has_attribute("open"));
        summary.click();
        assert!(details.has_attribute("open"));
        let save: HtmlElement = document.element(&paths["onSave"]).unwrap().unchecked_into();
        save.click();
        assert_eq!(document.drain_callbacks().len(), 1);
        values.insert(
            "sourceRefs".into(),
            vec!["cue-b", "new", "cue-a"].into_value(),
        );
        document
            .update_module_props_with_callbacks(values, host.owner.export_bindings().unwrap())
            .unwrap();
        let rows = container.query_selector_all("[data-meari-repeat]").unwrap();
        assert_eq!(rows.length(), 3);
        assert!(rows.item(2).unwrap().is_same_node(Some(&retained)));
        assert!(details.has_attribute("open"));
        assert!(
            document.last_error().is_none(),
            "{:?}",
            document.last_error()
        );
        document.close();
        container.remove();
    }
}

#[wasm_bindgen_test]
fn browser_repeat_callbacks_follow_retained_nodes_and_revoke_deleted_exports() {
    use meari::runtime::IntoValue;
    let entry: BuildPath = "list.tsx".to_owned().try_into().unwrap();
    let graph = meari::parse_module_graph(&entry, &BTreeMap::from([
        (entry.clone(), "import {Row} from './row';type Props={refs:string[];onText:(text:string)=>void;onPick:()=>void};export function List(props:Props){return <ul>{props.refs.map(reference => <Row key={reference} label={reference} onText={props.onText} onPick={props.onPick}/>)}</ul>;}".into()),
        ("row.tsx".to_owned().try_into().unwrap(), "type Props={label:string;onText:(text:string)=>void;onPick:()=>void};export function Row(props:Props){return <li><textarea value={props.label} onChange={event=>props.onText(event.currentTarget.value)}/><button onClick={()=>props.onPick()}>pick</button></li>;}".into()),
    ])).unwrap();
    let mut values = BTreeMap::from([
        ("refs".into(), vec!["a", "b"].into_value()),
        (
            "onText".into(),
            Value::CallbackReference(CallbackReference {
                binding: "onText".into(),
                signature: meari::domain::host_callbacks::CallbackSignature::Text,
            }),
        ),
        (
            "onPick".into(),
            Value::CallbackReference(CallbackReference {
                binding: "onPick".into(),
                signature: meari::domain::host_callbacks::CallbackSignature::Action,
            }),
        ),
    ]);
    let program =
        ModuleProgram::mount(graph, values.clone(), "web-repeat-callbacks".into(), 1).unwrap();
    let (mut document, mut host, container, _) = mount_program(program, "web-repeat-callbacks");
    let input: HtmlTextAreaElement = document.element("0/0/0").unwrap().unchecked_into();
    let button: HtmlElement = document.element("0/0/1").unwrap().unchecked_into();
    input.set_value("before🦀");
    input.dispatch_event(&Event::new("input").unwrap()).unwrap();
    let old = document.drain_callbacks().pop().unwrap();
    values.insert("refs".into(), vec!["b", "new", "a"].into_value());
    document
        .update_module_props_with_callbacks(values.clone(), host.owner.export_bindings().unwrap())
        .unwrap();
    assert!(
        document
            .element("0/2/0")
            .unwrap()
            .is_same_node(Some(input.as_ref()))
    );
    assert_eq!(
        host.owner
            .dispatch(&old.handle, old.invocation.arguments, &mut host.registry)
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    input.set_value("after🦀");
    input.dispatch_event(&Event::new("input").unwrap()).unwrap();
    let pending = document.drain_callbacks().pop().unwrap();
    assert_eq!(
        pending.invocation.arguments,
        CallbackArguments::Text("after🦀".into())
    );
    let mut invalid = values.clone();
    invalid.insert("refs".into(), vec!["same", "same"].into_value());
    assert!(
        document
            .update_module_props_with_callbacks(invalid, host.owner.export_bindings().unwrap())
            .is_err()
    );
    host.owner
        .dispatch(
            &pending.handle,
            pending.invocation.arguments,
            &mut host.registry,
        )
        .unwrap();
    button.click();
    let deleted = document.drain_callbacks().pop().unwrap();
    values.insert("refs".into(), vec!["b"].into_value());
    document
        .update_module_props_with_callbacks(values, host.owner.export_bindings().unwrap())
        .unwrap();
    assert_eq!(
        host.owner
            .dispatch(
                &deleted.handle,
                deleted.invocation.arguments,
                &mut host.registry
            )
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    input.dispatch_event(&Event::new("input").unwrap()).unwrap();
    button.click();
    assert!(document.drain_callbacks().is_empty());
    assert!(
        document.last_error().is_none(),
        "{:?}",
        document.last_error()
    );
    document.close();
    container.remove();
}

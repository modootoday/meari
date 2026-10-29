//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use meari::{
    application::{host_callbacks::HostCallbacks, source_modules::ModuleProgram},
    domain::{
        PropType, Value,
        build::{BuildPath, ContentHash, TargetKind},
        host_callbacks::CallbackReference,
    },
};
use meari_native::{domain::Viewport, infrastructure::document::InteractiveDocument};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

pub(super) fn hash(value: &str) -> ContentHash {
    ContentHash::of(value.as_bytes())
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

pub(super) fn props(source: &str) -> BTreeMap<String, Value> {
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

pub(super) fn program(source: &str, values: BTreeMap<String, Value>) -> ModuleProgram {
    let entry: BuildPath = "view.tsx".to_owned().try_into().unwrap();
    let graph =
        meari::parse_module_graph(&entry, &BTreeMap::from([(entry.clone(), source.into())]))
            .unwrap();
    ModuleProgram::mount(graph, values, "callback-document".into(), 1).unwrap()
}

pub(super) fn mount(source: &str) -> (InteractiveDocument, Host) {
    let values = props(source);
    let program = program(source, values.clone());
    mount_program(program, values)
}

pub(super) fn mount_program(
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

pub(super) fn editor(document: &InteractiveDocument) -> (String, blitz_dom::NodeId) {
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

pub(super) fn text(document: &InteractiveDocument, node: blitz_dom::NodeId) -> String {
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

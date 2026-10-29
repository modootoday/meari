//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use crate::domain::Viewport;
use meari::{
    application::{host_callbacks::HostCallbacks, source_modules::ModuleProgram},
    domain::{
        Value,
        build::{BuildPath, ContentHash, TargetKind},
        host_callbacks::{CallbackReference, CallbackSignature},
        plugins::PluginScope,
    },
};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

pub(super) fn hash(value: &str) -> ContentHash {
    ContentHash::of(value.as_bytes())
}

impl PluginAdapter for Adapter {
    fn manifest(&self) -> &PluginManifest {
        &self.0
    }
    fn validate_payload(
        &self,
        _: &str,
        _: PayloadDirection,
        _: &PluginPayload,
    ) -> Result<(), PluginError> {
        Ok(())
    }
    fn dispatch(&mut self, _: &PluginInvocation) -> Result<PluginDispatch, PluginError> {
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

pub(super) fn props(checked: bool, disabled: bool) -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("checked".into(), Value::Bool(checked)),
        ("disabled".into(), Value::Bool(disabled)),
        (
            "change".into(),
            Value::CallbackReference(CallbackReference {
                binding: "change".into(),
                signature: CallbackSignature::Boolean,
            }),
        ),
    ])
}

pub(super) fn mount() -> (
    InteractiveDocument,
    HostCallbacks,
    PluginRegistry,
    Rc<Cell<usize>>,
) {
    mount_source(
        "type Props={checked:boolean;disabled:boolean;change:(value:boolean)=>void};export function Panel(props:Props){return <Switch checked={props.checked} disabled={props.disabled} onCheckedChange={value=>props.change(value)} aria-label=\"test\"/>;}",
    )
}

pub(super) fn program_source(source: &str) -> ModuleProgram {
    let entry: BuildPath = "panel.tsx".to_owned().try_into().unwrap();
    let graph = meari::parse_module_graph(
        &entry,
        &BTreeMap::from([
            (entry.clone(), source.into()),
            (
                "dock.tsx".to_owned().try_into().unwrap(),
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../meari/fixtures/node-slots/studio/inspector-dock.tsx"
                ))
                .into(),
            ),
        ]),
    )
    .unwrap();
    ModuleProgram::mount(graph, props(false, false), "boolean-adapter".into(), 1).unwrap()
}

pub(super) fn mount_source(
    source: &str,
) -> (
    InteractiveDocument,
    HostCallbacks,
    PluginRegistry,
    Rc<Cell<usize>>,
) {
    let program = program_source(source);
    let projection = program.project().unwrap();
    let scope = PluginScope {
        instance: projection.instances["root"].instance.clone(),
        source_generation: 7,
    };
    let mut registry =
        PluginRegistry::new(hash("registry"), TargetKind::Native, scope.clone(), 8).unwrap();
    let manifest = PluginManifest {
        api_version: PLUGIN_API_VERSION,
        id: "test.boolean".into(),
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
            Box::new(Adapter(manifest)),
            BTreeSet::from(["intent".into()]),
        )
        .unwrap();
    let mut host = HostCallbacks::new(scope.clone());
    let calls = Rc::new(Cell::new(0));
    let produced = calls.clone();
    host.bind(
        "change".into(),
        CallbackSignature::Boolean,
        registration,
        "intent".into(),
        move |arguments| {
            produced.set(produced.get() + 1);
            Ok(PluginPayload {
                schema: hash("request"),
                value: serde_json::to_value(arguments).unwrap(),
            })
        },
    )
    .unwrap();
    let document = InteractiveDocument::from_module_program_with_callbacks(
        program,
        "button{width:100px;height:32px}",
        Viewport {
            width: 640,
            height: 480,
            dark: false,
        },
        None,
        7,
        |received| {
            assert_eq!(received, scope);
            Ok(host.export_bindings().unwrap())
        },
    )
    .unwrap();
    (document, host, registry, calls)
}

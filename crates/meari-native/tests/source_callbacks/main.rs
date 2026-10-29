use meari::{
    application::{host_callbacks::HostCallbacks, plugins::*, source_modules::ModuleProgram},
    domain::{
        PropType, Value,
        build::BuildPath,
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
struct Recorder {
    manifest: PluginManifest,
    received: Rc<RefCell<Vec<serde_json::Value>>>,
}
struct Host {
    callbacks: HostCallbacks,
    registry: PluginRegistry,
    received: Rc<RefCell<Vec<serde_json::Value>>>,
}

mod harness;
use self::harness::*;
mod graph_mounts;

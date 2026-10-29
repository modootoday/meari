use super::node_slots::NodeSlotFactory;
mod hooks;
use hooks::HookPass;
pub use hooks::{
    HookModuleFailure, HookModulePublicationFailure, HookModulePublicationProblem,
    PreparedHookModule,
};
#[cfg(test)]
mod slot_tests;
use super::{ComponentReference, ModuleDiagnostic, SourceModuleGraph};
use crate::domain::node_slots::SlotReference;
use crate::{
    application::{
        component_state::{ComponentState, StateCommit},
        hook_document::{BoundHandler, DocumentStamp, PreparedDocument},
        state_program::{StateProgram, StateProjection},
    },
    domain::{
        Component, Node, Span, Value,
        build::{BuildPath, ContentHash},
        component_state::InstanceHandle,
        input_event::InputStateBinding,
        source_modules::SourceComponentModule,
        state_program::StateEvent,
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ModuleOrigin {
    pub module: BuildPath,
    pub instance: String,
    pub source_path: String,
    pub span: Span,
}

#[derive(Clone)]
struct MountedModule {
    reference: ComponentReference,
    bound: Component,
    state: MountedState,
}

#[derive(Clone)]
enum MountedState {
    Scalar(Box<StateProgram>),
    Hook(InstanceHandle),
}
impl MountedState {
    fn scalar_mut(&mut self) -> Result<&mut StateProgram, &'static str> {
        match self {
            Self::Scalar(program) => Ok(program),
            Self::Hook(_) => Err("hook module requires prepared document dispatch"),
        }
    }
}

#[derive(Clone, Debug)]
struct EventBinding {
    instance: String,
    event: StateEvent,
    handler: Option<BoundHandler>,
}
#[derive(Clone, Debug)]
struct InputBinding {
    instance: String,
    binding: InputStateBinding,
}

#[derive(Clone, Debug)]
pub struct ModuleProjection {
    pub component: Component,
    pub origins: BTreeMap<String, ModuleOrigin>,
    pub calls: BTreeMap<String, ModuleOrigin>,
    pub instances: BTreeMap<String, StateCommit>,
    hook_instances: BTreeMap<String, InstanceHandle>,
    events: BTreeMap<String, EventBinding>,
    inputs: BTreeMap<String, InputBinding>,
    callbacks: BTreeMap<String, crate::domain::host_callbacks::BoundCallback>,
}

impl ModuleProjection {
    pub fn instance(&self, key: &str) -> Option<&InstanceHandle> {
        self.hook_instances
            .get(key)
            .or_else(|| self.instances.get(key).map(|state| &state.instance))
    }
    pub fn uses_hook_document(&self) -> bool {
        !self.hook_instances.is_empty()
    }
    pub fn hook_action(
        &self,
        path: &str,
    ) -> Option<crate::application::hook_document::BoundAction> {
        Some(self.events.get(path)?.handler.as_ref()?.action())
    }
    pub fn callback_bindings(
        &self,
    ) -> BTreeMap<String, crate::domain::host_callbacks::BoundCallback> {
        self.callbacks.clone()
    }
    pub fn input_bindings(&self) -> BTreeMap<String, InputStateBinding> {
        self.inputs
            .iter()
            .map(|(path, input)| (path.clone(), input.binding.clone()))
            .collect()
    }
    pub fn event_paths(&self) -> impl Iterator<Item = &String> {
        self.events.keys()
    }
    pub fn input_paths(&self) -> impl Iterator<Item = &String> {
        self.inputs.keys()
    }
}

/// Owns one state program per source component instance; projections are renderer inputs.
#[derive(Clone)]
pub struct ModuleProgram {
    graph: SourceModuleGraph,
    props: BTreeMap<String, Value>,
    document: String,
    epoch: u64,
    mounted: BTreeMap<String, MountedModule>,
    projection: Option<ModuleProjection>,
    slot_owner: u64,
    active_slots: BTreeSet<u64>,
    slot_generation: u64,
    hook_registry: Option<ComponentState>,
    hook_stamp: Option<DocumentStamp>,
}

struct ProjectionContext<'a> {
    render_stack: Vec<String>,
    hooks: Option<HookPass<'a>>,
    slot_owner: u64,
    active: &'a BTreeSet<u64>,
    generation: u64,
    owned: BTreeSet<u64>,
    movable: BTreeSet<String>,
    slots: BTreeMap<SlotReference, CapturedSlot>,
    graph: &'a SourceModuleGraph,
    document: &'a str,
    epoch: u64,
    previous: &'a BTreeMap<String, MountedModule>,
    next: BTreeMap<String, MountedModule>,
    origins: BTreeMap<String, ModuleOrigin>,
    calls: BTreeMap<String, ModuleOrigin>,
    events: BTreeMap<String, EventBinding>,
    inputs: BTreeMap<String, InputBinding>,
    callbacks: BTreeMap<String, crate::domain::host_callbacks::BoundCallback>,
    nodes: usize,
}
struct CapturedSlot {
    nodes: Vec<(Node, String)>,
    module: Rc<SourceComponentModule>,
    reference: ComponentReference,
    instance: String,
    projection: Rc<StateProjection>,
}

mod lifecycle;
use self::lifecycle::*;
mod components;
mod slot_expansion;

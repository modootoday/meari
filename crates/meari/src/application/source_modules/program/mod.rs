use super::node_slots::{self, NodeSlotFactory};
mod hooks;
use hooks::HookPass;
pub use hooks::{
    HookModuleFailure, HookModulePublicationFailure, HookModulePublicationProblem,
    PreparedHookModule,
};
#[cfg(test)]
mod slot_tests;
use super::{ComponentReference, ModuleDiagnostic, SourceModuleGraph};
use crate::domain::node_slots::{NodeSlot, SlotReference};
use crate::{
    application::{
        binding,
        component_state::{ComponentState, StateCommit},
        hook_document::{BoundHandler, DocumentStamp, PreparedDocument},
        state_program::{ReloadPolicy, StateProgram, StateProjection},
    },
    domain::{
        Component, Node, NodeKind, Span, Value,
        build::{BuildPath, ContentHash},
        component_state::InstanceHandle,
        input_event::{InputPayload, InputStateBinding},
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

impl ModuleProgram {
    pub fn definitions(&self) -> impl Iterator<Item = &Component> {
        self.graph.modules.values().map(|module| &module.component)
    }
    pub fn mount(
        graph: SourceModuleGraph,
        props: BTreeMap<String, Value>,
        document: String,
        epoch: u64,
    ) -> Result<Self, ModuleDiagnostic> {
        Self::mount_with_node_slots(graph, document, epoch, |_| Ok(props))
    }
    pub fn node_slots(&self) -> NodeSlotFactory<'_> {
        NodeSlotFactory {
            graph: &self.graph,
            owner: self.slot_owner,
            active: &self.active_slots,
            generation: self.slot_generation,
        }
    }
    pub fn mount_with_node_slots(
        graph: SourceModuleGraph,
        document: String,
        epoch: u64,
        prepare: impl FnOnce(&NodeSlotFactory<'_>) -> Result<BTreeMap<String, Value>, String>,
    ) -> Result<Self, ModuleDiagnostic> {
        let mut program = Self::configure(graph, document, epoch, prepare)?;
        program.refresh()?;
        Ok(program)
    }

    fn configure(
        graph: SourceModuleGraph,
        document: String,
        epoch: u64,
        prepare: impl FnOnce(&NodeSlotFactory<'_>) -> Result<BTreeMap<String, Value>, String>,
    ) -> Result<Self, ModuleDiagnostic> {
        graph
            .validate()
            .map_err(|message| failure(&graph.entry.module, Span::default(), message))?;
        if !crate::domain::component_state::identifier(&document) || epoch == 0 {
            return Err(failure(
                &graph.entry.module,
                Span::default(),
                "module document scope/epoch required",
            ));
        }
        let slot_owner = node_slots::identity()
            .map_err(|error| failure(&graph.entry.module, Span::default(), error))?;
        let active_slots = BTreeSet::new();
        let props = prepare(&NodeSlotFactory {
            graph: &graph,
            owner: slot_owner,
            active: &active_slots,
            generation: 1,
        })
        .map_err(|error| failure(&graph.entry.module, Span::default(), error))?;
        let program = Self {
            graph,
            props,
            document,
            epoch,
            mounted: BTreeMap::new(),
            projection: None,
            slot_owner,
            active_slots,
            slot_generation: 1,
            hook_registry: None,
            hook_stamp: None,
        };
        Ok(program)
    }

    pub fn project(&self) -> Result<&ModuleProjection, ModuleDiagnostic> {
        self.projection.as_ref().ok_or_else(|| {
            failure(
                &self.graph.entry.module,
                Span::default(),
                "module projection missing",
            )
        })
    }

    pub fn update_props(
        &mut self,
        props: BTreeMap<String, Value>,
    ) -> Result<ModuleProjection, ModuleDiagnostic> {
        let mut staged = self.clone();
        staged.props = props;
        staged.refresh()?;
        let projection = staged.project()?.clone();
        *self = staged;
        Ok(projection)
    }

    pub fn activate(&mut self, path: &str) -> Result<ModuleProjection, ModuleDiagnostic> {
        let event = self.project()?.events.get(path).cloned().ok_or_else(|| {
            failure(
                &self.graph.entry.module,
                Span::default(),
                "module event path missing",
            )
        })?;
        let mut staged = self.clone();
        let mounted = staged.mounted.get_mut(&event.instance).ok_or_else(|| {
            failure(
                &self.graph.entry.module,
                Span::default(),
                "module event instance missing",
            )
        })?;
        mounted
            .state
            .scalar_mut()
            .map_err(|message| failure(&mounted.reference.module, Span::default(), message))?
            .dispatch(&event.event)
            .map_err(|message| failure(&mounted.reference.module, Span::default(), message))?;
        staged.refresh()?;
        let projection = staged.project()?.clone();
        *self = staged;
        Ok(projection)
    }

    pub fn input(
        &mut self,
        path: &str,
        payload: &InputPayload,
    ) -> Result<ModuleProjection, ModuleDiagnostic> {
        let input = self.project()?.inputs.get(path).cloned().ok_or_else(|| {
            failure(
                &self.graph.entry.module,
                Span::default(),
                "module input path missing",
            )
        })?;
        let mut staged = self.clone();
        let mounted = staged.mounted.get_mut(&input.instance).ok_or_else(|| {
            failure(
                &self.graph.entry.module,
                Span::default(),
                "module input instance missing",
            )
        })?;
        let handler = mounted
            .state
            .scalar_mut()
            .map_err(|message| failure(&mounted.reference.module, Span::default(), message))?
            .input_handler(input.binding)
            .map_err(|message| failure(&mounted.reference.module, Span::default(), message))?;
        mounted
            .state
            .scalar_mut()
            .map_err(|message| failure(&mounted.reference.module, Span::default(), message))?
            .dispatch_input(&handler, payload)
            .map_err(|message| failure(&mounted.reference.module, Span::default(), message))?;
        staged.refresh()?;
        let projection = staged.project()?.clone();
        *self = staged;
        Ok(projection)
    }

    fn refresh(&mut self) -> Result<(), ModuleDiagnostic> {
        self.refresh_with_hooks(None)
    }

    fn refresh_with_hooks(
        &mut self,
        prepared: Option<&mut PreparedDocument>,
    ) -> Result<(), ModuleDiagnostic> {
        let hooks = match (self.hook_registry.as_mut(), prepared) {
            (None, None) => None,
            (Some(registry), Some(prepared)) => Some(HookPass { registry, prepared }),
            _ => {
                return Err(failure(
                    &self.graph.entry.module,
                    Span::default(),
                    "hook module projection requires its prepared document",
                ));
            }
        };
        let active_slots = node_slots::validate_runtime(
            &self.props,
            self.slot_owner,
            &self.active_slots,
            self.slot_generation,
        )
        .map_err(|error| failure(&self.graph.entry.module, Span::default(), error))?;
        let next_generation = self.slot_generation.checked_add(1).ok_or_else(|| {
            failure(
                &self.graph.entry.module,
                Span::default(),
                "node slot generation exhausted",
            )
        })?;
        let mut context = ProjectionContext {
            hooks,
            graph: &self.graph,
            document: &self.document,
            epoch: self.epoch,
            previous: &self.mounted,
            next: BTreeMap::new(),
            origins: BTreeMap::new(),
            calls: BTreeMap::new(),
            events: BTreeMap::new(),
            inputs: BTreeMap::new(),
            callbacks: BTreeMap::new(),
            nodes: 0,
            render_stack: Vec::new(),
            slot_owner: self.slot_owner,
            active: &self.active_slots,
            generation: self.slot_generation,
            owned: BTreeSet::new(),
            movable: BTreeSet::new(),
            slots: BTreeMap::new(),
        };
        let root = context.component(&self.graph.entry, &self.props, "root", "0", None, 0)?;
        let definition = &self
            .graph
            .definition(&self.graph.entry)
            .map_err(|message| failure(&self.graph.entry.module, Span::default(), message))?
            .component;
        let mut instances = BTreeMap::new();
        let mut hook_instances = BTreeMap::new();
        for (instance, mounted) in &context.next {
            match &mounted.state {
                MountedState::Scalar(program) => {
                    instances.insert(
                        instance.clone(),
                        program.snapshot().map_err(|message| {
                            failure(&mounted.reference.module, Span::default(), message)
                        })?,
                    );
                }
                MountedState::Hook(handle) => {
                    hook_instances.insert(instance.clone(), handle.clone());
                }
            }
        }
        if let Some(pass) = &mut context.hooks {
            let active = hook_instances.values().cloned().collect();
            pass.registry
                .retain_instances(&active)
                .map_err(|message| failure(&self.graph.entry.module, Span::default(), message))?;
            pass.prepared
                .retain_instances(&active)
                .map_err(|diagnostic| ModuleDiagnostic {
                    file: self.graph.entry.module.clone(),
                    diagnostic,
                })?;
            self.hook_stamp = Some(pass.prepared.stamp());
        }
        self.projection = Some(ModuleProjection {
            component: Component {
                name: definition.name.clone(),
                client: definition.client,
                props: Vec::new(),
                states: Vec::new(),
                hook_order: Vec::new(),
                root,
            },
            origins: context.origins,
            calls: context.calls,
            events: context.events,
            inputs: context.inputs,
            callbacks: context.callbacks,
            instances,
            hook_instances,
        });
        self.mounted = context.next;
        self.active_slots = active_slots;
        self.slot_generation = next_generation;
        Ok(())
    }
}

fn failure(file: &BuildPath, span: Span, message: impl Into<String>) -> ModuleDiagnostic {
    ModuleDiagnostic {
        file: file.clone(),
        diagnostic: crate::domain::Diagnostic::unsupported(span, message),
    }
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

impl ProjectionContext<'_> {
    fn component(
        &mut self,
        reference: &ComponentReference,
        props: &BTreeMap<String, Value>,
        instance: &str,
        output: &str,
        parent: Option<&str>,
        depth: usize,
    ) -> Result<Node, ModuleDiagnostic> {
        let module = Rc::new(
            self.graph
                .definition(reference)
                .map_err(|message| failure(&reference.module, Span::default(), message))?
                .clone(),
        );
        if depth > 128 || self.next.len() >= 16384 {
            return Err(failure(
                &reference.module,
                module.component.root.span,
                "component instance/depth budget exceeded",
            ));
        }
        let scope = ContentHash::of(
            &serde_json::to_vec(&(self.document, self.epoch, instance, reference))
                .map_err(|error| failure(&reference.module, Span::default(), error.to_string()))?,
        );
        let bound = binding::bind_with_scope(&module.component, props, Some(scope.as_str()))
            .map_err(|message| failure(&reference.module, module.component.root.span, message))?;
        let (state, projection) = if let Some(pass) = &mut self.hooks {
            let parent = parent
                .map(|parent| {
                    match &self
                        .next
                        .get(parent)
                        .ok_or_else(|| {
                            failure(
                                &reference.module,
                                Span::default(),
                                "hook parent instance missing",
                            )
                        })?
                        .state
                    {
                        MountedState::Hook(handle) => Ok(handle.clone()),
                        _ => Err(failure(
                            &reference.module,
                            Span::default(),
                            "mixed hook/scalar module ownership",
                        )),
                    }
                })
                .transpose()?;
            let identity =
                ContentHash::of(&serde_json::to_vec(reference).map_err(|error| {
                    failure(&reference.module, Span::default(), error.to_string())
                })?);
            let handle = pass
                .registry
                .mount(
                    parent.as_ref(),
                    crate::domain::component_state::InstanceKey::Explicit(
                        ContentHash::of(instance.as_bytes()).as_str().into(),
                    ),
                    identity.as_str().into(),
                    crate::domain::component_state::StateSchema {
                        revision: "hook-module-instance-v1".into(),
                        slots: BTreeMap::new(),
                    },
                )
                .map_err(|message| failure(&reference.module, bound.root.span, message))?;
            pass.prepared
                .ensure_instance(crate::application::hook_document::Mount {
                    instance: handle.clone(),
                    definition: bound.clone(),
                    definition_revision: 1,
                })
                .map_err(|diagnostic| ModuleDiagnostic {
                    file: reference.module.clone(),
                    diagnostic,
                })?;
            let projection = pass
                .prepared
                .project(&handle, &bound)
                .map_err(|diagnostic| ModuleDiagnostic {
                    file: reference.module.clone(),
                    diagnostic,
                })?;
            if !projection.inputs.is_empty() {
                return Err(failure(
                    &reference.module,
                    bound.root.span,
                    "hook module inputs require prepared payload dispatch",
                ));
            }
            (MountedState::Hook(handle), Rc::new(projection))
        } else {
            let program = if let Some(previous) = self
                .previous
                .get(instance)
                .filter(|previous| previous.reference == *reference)
            {
                let MountedState::Scalar(previous_program) = &previous.state else {
                    return Err(failure(
                        &reference.module,
                        Span::default(),
                        "hook module requires prepared projection",
                    ));
                };
                let mut program = previous_program.as_ref().clone();
                if !previous.bound.semantic_eq(&bound) {
                    let replacement = program
                        .prepare_replacement(bound.clone(), ReloadPolicy::PreserveCompatible)
                        .map_err(|message| {
                            failure(&reference.module, module.component.root.span, message)
                        })?;
                    program.commit_replacement(replacement).map_err(|message| {
                        failure(&reference.module, module.component.root.span, message)
                    })?;
                }
                program
            } else {
                StateProgram::mount(
                    bound.clone(),
                    format!(
                        "module-{}-{}",
                        scope.as_str(),
                        node_slots::identity().map_err(|message| {
                            failure(&reference.module, module.component.root.span, message)
                        })?
                    ),
                    self.epoch,
                )
                .map_err(|message| {
                    failure(&reference.module, module.component.root.span, message)
                })?
            };
            let projection = Rc::new(program.project().map_err(|message| {
                failure(&reference.module, module.component.root.span, message)
            })?);
            (MountedState::Scalar(Box::new(program)), projection)
        };
        self.next.insert(
            instance.into(),
            MountedModule {
                reference: reference.clone(),
                bound,
                state,
            },
        );
        self.render_stack.push(instance.into());
        let nodes = self.node(
            projection.component.root.clone(),
            &module,
            reference,
            instance,
            "0",
            "0",
            output,
            &projection,
            depth,
        );
        self.render_stack.pop();
        let mut nodes = nodes?;
        if nodes.len() != 1 {
            return Err(failure(
                &reference.module,
                module.component.root.span,
                "component must project exactly one root node",
            ));
        }
        Ok(nodes.remove(0))
    }

    #[allow(clippy::too_many_arguments)]
    fn node(
        &mut self,
        mut node: Node,
        module: &Rc<SourceComponentModule>,
        reference: &ComponentReference,
        instance: &str,
        local: &str,
        placement: &str,
        output: &str,
        projection: &Rc<StateProjection>,
        depth: usize,
    ) -> Result<Vec<Node>, ModuleDiagnostic> {
        self.nodes += 1;
        if self.nodes > 100000 || depth > 128 {
            return Err(failure(
                &reference.module,
                node.span,
                "module projection node/depth budget exceeded",
            ));
        }
        if let NodeKind::Text(Value::NodeSlot(slot)) = &node.kind {
            match slot {
                NodeSlot::Empty => return Ok(Vec::new()),
                NodeSlot::Content(nodes) => {
                    let captured = CapturedSlot {
                        nodes: nodes
                            .iter()
                            .enumerate()
                            .map(|(index, node)| (node.clone(), format!("{local}/slot/{index}")))
                            .collect(),
                        module: module.clone(),
                        reference: reference.clone(),
                        instance: instance.into(),
                        projection: projection.clone(),
                    };
                    return self.expand_slot(captured, placement, output, depth + 1);
                }
                NodeSlot::Captured(slot) => {
                    if slot.owner != self.slot_owner {
                        return Err(failure(
                            &reference.module,
                            node.span,
                            "foreign node slot owner",
                        ));
                    }
                    let captured = self.slots.remove(slot).ok_or_else(|| {
                        failure(
                            &reference.module,
                            node.span,
                            "stale or multiply consumed node slot",
                        )
                    })?;
                    return self.expand_slot(captured, placement, output, depth + 1);
                }
                NodeSlot::Owned(slot) => {
                    if slot.owner != self.slot_owner
                        || (!self.active.contains(&slot.nonce)
                            && slot.generation != self.generation)
                    {
                        return Err(failure(
                            &reference.module,
                            node.span,
                            "foreign or retired node slot lease",
                        ));
                    }
                    if self.owned.len() >= 16384 {
                        return Err(failure(
                            &reference.module,
                            node.span,
                            "active node slot budget exceeded",
                        ));
                    }
                    if !self.owned.insert(slot.nonce) {
                        return Err(failure(
                            &reference.module,
                            node.span,
                            "owned node slot cannot be inserted more than once",
                        ));
                    }
                    let key = ContentHash::of(slot.key.as_bytes());
                    let parent = self.render_stack.last().cloned().ok_or_else(|| {
                        failure(&reference.module, node.span, "slot render parent missing")
                    })?;
                    let child = if slot.movable {
                        let container = placement.rsplit_once('/').map_or("", |(parent, _)| parent);
                        let child = format!("{parent}@movable-slot:{container}/{}", key.as_str());
                        if !self.movable.insert(child.clone()) {
                            return Err(failure(
                                &reference.module,
                                node.span,
                                "movable slot key is duplicated within its source parent",
                            ));
                        }
                        child
                    } else {
                        format!("{parent}@slot:{placement}/{}", key.as_str())
                    };
                    self.calls.insert(
                        child.clone(),
                        ModuleOrigin {
                            module: reference.module.clone(),
                            instance: instance.into(),
                            source_path: local.into(),
                            span: node.span,
                        },
                    );
                    let mut rendered = self.component(
                        &slot.reference,
                        &slot.props,
                        &child,
                        output,
                        Some(&parent),
                        depth + 1,
                    )?;
                    if let NodeKind::Element { attributes, .. } = &mut rendered.kind {
                        if attributes.iter().any(|attribute| {
                            attribute.name == crate::application::repeat::IDENTITY_ATTRIBUTE
                        }) {
                            return Err(failure(
                                &reference.module,
                                node.span,
                                "owned node root identity attribute is reserved",
                            ));
                        }
                        let identity = ContentHash::of(
                            &serde_json::to_vec(&(&child, &slot.reference)).map_err(|error| {
                                failure(&reference.module, node.span, error.to_string())
                            })?,
                        );
                        attributes.push(crate::domain::Attribute {
                            name: crate::application::repeat::IDENTITY_ATTRIBUTE.into(),
                            value: Value::String(identity.as_str().into()),
                        });
                    }
                    return Ok(vec![rendered]);
                }
            }
        }
        let origin = ModuleOrigin {
            module: reference.module.clone(),
            instance: instance.into(),
            source_path: local.into(),
            span: node.span,
        };
        if let NodeKind::Element {
            tag,
            attributes,
            children,
        } = &mut node.kind
        {
            if let Some(import) = module.imports.get(tag) {
                if projection.events.contains_key(local) || projection.inputs.contains_key(local) {
                    return Err(failure(
                        &reference.module,
                        node.span,
                        "source component callback props require a typed callback contract",
                    ));
                }
                let mut props: BTreeMap<String, Value> = attributes
                    .iter()
                    .filter(|attribute| {
                        attribute.name != crate::application::repeat::IDENTITY_ATTRIBUTE
                    })
                    .map(|attribute| (attribute.name.clone(), attribute.value.clone()))
                    .collect();
                for (name, value) in &mut props {
                    if let Value::NodeSlot(NodeSlot::Content(nodes)) = value {
                        let nodes = nodes
                            .iter()
                            .enumerate()
                            .map(|(index, node)| {
                                (node.clone(), format!("{local}/slot:{name}/{index}"))
                            })
                            .collect();
                        *value = self.capture_slot(
                            format!("{instance}/{local}:{name}"),
                            nodes,
                            module,
                            reference,
                            instance,
                            projection,
                        )?;
                    }
                }
                if !children.is_empty() {
                    if props.contains_key("children") {
                        return Err(failure(
                            &reference.module,
                            node.span,
                            "children supplied both as attribute and nested nodes",
                        ));
                    }
                    let definition = self
                        .graph
                        .definition(&import.target)
                        .map_err(|error| failure(&reference.module, node.span, error))?;
                    if !definition.component.props.iter().any(|prop| {
                        prop.name == "children" && prop.kind == crate::domain::PropType::Node
                    }) {
                        return Err(failure(
                            &reference.module,
                            node.span,
                            "source component children require a typed slot contract",
                        ));
                    }
                    let nodes = children
                        .iter()
                        .enumerate()
                        .map(|(index, node)| (node.clone(), format!("{local}/{index}")))
                        .collect();
                    let value = self.capture_slot(
                        format!("{instance}/{local}:children"),
                        nodes,
                        module,
                        reference,
                        instance,
                        projection,
                    )?;
                    props.insert("children".into(), value);
                }
                let stable = attributes.iter().find_map(|attribute| {
                    if attribute.name == crate::application::repeat::IDENTITY_ATTRIBUTE
                        && let Value::String(identity) = &attribute.value
                    {
                        return Some(identity.as_str());
                    }
                    None
                });
                let parent = self.render_stack.last().cloned().ok_or_else(|| {
                    failure(
                        &reference.module,
                        node.span,
                        "component render parent missing",
                    )
                })?;
                let child = format!("{parent}@{}", stable.unwrap_or(placement));
                self.calls.insert(child.clone(), origin);
                let mut result = self.component(
                    &import.target,
                    &props,
                    &child,
                    output,
                    Some(&parent),
                    depth + 1,
                )?;
                if let Some(stable) = stable
                    && let NodeKind::Element { attributes, .. } = &mut result.kind
                {
                    attributes.push(crate::domain::Attribute {
                        name: crate::application::repeat::IDENTITY_ATTRIBUTE.into(),
                        value: Value::String(stable.into()),
                    });
                }
                return Ok(vec![result]);
            }
            for (index, child) in std::mem::take(children).into_iter().enumerate() {
                let expanded = self.node(
                    child,
                    module,
                    reference,
                    instance,
                    &format!("{local}/{index}"),
                    &format!("{placement}/{index}"),
                    &format!("{output}/{}", children.len()),
                    projection,
                    depth + 1,
                )?;
                children.extend(expanded);
            }
        }
        if let Some(event) = projection.events.get(local) {
            let handler = if let Some(pass) = &self.hooks {
                let MountedState::Hook(handle) = &self
                    .next
                    .get(instance)
                    .ok_or_else(|| {
                        failure(&reference.module, node.span, "hook event instance missing")
                    })?
                    .state
                else {
                    return Err(failure(
                        &reference.module,
                        node.span,
                        "hook event owner missing",
                    ));
                };
                Some(
                    pass.prepared
                        .handler(handle, event.clone())
                        .map_err(|diagnostic| ModuleDiagnostic {
                            file: reference.module.clone(),
                            diagnostic,
                        })?,
                )
            } else {
                None
            };
            self.events.insert(
                output.into(),
                EventBinding {
                    instance: instance.into(),
                    event: event.clone(),
                    handler,
                },
            );
        }
        if let Some(binding) = projection.inputs.get(local) {
            self.inputs.insert(
                output.into(),
                InputBinding {
                    instance: instance.into(),
                    binding: binding.clone(),
                },
            );
        }
        if let Some(binding) = projection.callbacks.get(local) {
            self.callbacks.insert(output.into(), binding.clone());
        }
        let commit_local =
            crate::application::host_callbacks::DocumentCallbacks::value_commit_key(local);
        if let Some(binding) = projection.callbacks.get(&commit_local) {
            self.callbacks.insert(
                crate::application::host_callbacks::DocumentCallbacks::value_commit_key(output),
                binding.clone(),
            );
        }
        self.origins.insert(output.into(), origin);
        Ok(vec![node])
    }
    #[allow(clippy::too_many_arguments)]
    fn capture_slot(
        &mut self,
        identity: String,
        nodes: Vec<(Node, String)>,
        module: &Rc<SourceComponentModule>,
        reference: &ComponentReference,
        instance: &str,
        projection: &Rc<StateProjection>,
    ) -> Result<Value, ModuleDiagnostic> {
        if self.slots.len() >= 16384 {
            return Err(failure(
                &reference.module,
                Span::default(),
                "node slot capture budget exceeded",
            ));
        }
        let slot = SlotReference {
            owner: self.slot_owner,
            identity,
        };
        let captured = CapturedSlot {
            nodes,
            module: module.clone(),
            reference: reference.clone(),
            instance: instance.into(),
            projection: projection.clone(),
        };
        if self.slots.insert(slot.clone(), captured).is_some() {
            return Err(failure(
                &reference.module,
                Span::default(),
                "duplicate node slot source identity",
            ));
        }
        Ok(Value::NodeSlot(NodeSlot::Captured(slot)))
    }
    fn expand_slot(
        &mut self,
        captured: CapturedSlot,
        placement: &str,
        output: &str,
        depth: usize,
    ) -> Result<Vec<Node>, ModuleDiagnostic> {
        let mut expanded = Vec::new();
        for (index, (node, local)) in captured.nodes.into_iter().enumerate() {
            let path = if expanded.is_empty() {
                output.to_owned()
            } else {
                let (parent, index) = output.rsplit_once('/').ok_or_else(|| {
                    failure(
                        &captured.reference.module,
                        node.span,
                        "multiple slot roots require a containing element",
                    )
                })?;
                let index = index.parse::<usize>().map_err(|_| {
                    failure(
                        &captured.reference.module,
                        node.span,
                        "invalid slot placement",
                    )
                })?;
                format!("{parent}/{}", index + expanded.len())
            };
            expanded.extend(self.node(
                node,
                &captured.module,
                &captured.reference,
                &captured.instance,
                &local,
                &format!("{placement}/slot/{index}"),
                &path,
                &captured.projection,
                depth,
            )?);
        }
        Ok(expanded)
    }
}

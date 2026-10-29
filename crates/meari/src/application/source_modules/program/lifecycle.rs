//! Split from the parent module to keep files under the 600-line structure limit.
use super::super::node_slots::{self, NodeSlotFactory};
use super::super::{ModuleDiagnostic, SourceModuleGraph};
use super::*;
use crate::{
    application::hook_document::PreparedDocument,
    domain::{Component, Span, Value, build::BuildPath, input_event::InputPayload},
};
use hooks::HookPass;
use std::collections::{BTreeMap, BTreeSet};

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

    pub(super) fn configure(
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

    pub(super) fn refresh(&mut self) -> Result<(), ModuleDiagnostic> {
        self.refresh_with_hooks(None)
    }

    pub(super) fn refresh_with_hooks(
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

pub(super) fn failure(
    file: &BuildPath,
    span: Span,
    message: impl Into<String>,
) -> ModuleDiagnostic {
    ModuleDiagnostic {
        file: file.clone(),
        diagnostic: crate::domain::Diagnostic::unsupported(span, message),
    }
}

use super::*;
use crate::application::hook_document::{self, HookDocument};

pub(super) struct HookPass<'a> {
    pub registry: &'a mut ComponentState,
    pub prepared: &'a mut PreparedDocument,
}

/// A structural projection and its one unpublished document heap candidate.
pub struct PreparedHookModule {
    program: ModuleProgram,
    state: PreparedDocument,
}
impl PreparedHookModule {
    /// `apply` must include the adapter's real patch/recovery and final receipt checks.
    pub fn publish_with<R, E>(
        self,
        hooks: &mut HookDocument,
        apply: impl FnOnce(&ModuleProgram) -> Result<R, E>,
    ) -> Result<(ModuleProgram, R), Box<HookModulePublicationFailure<E>>> {
        let Self { program, state } = self;
        let guard = match hooks.prepare_publication(state) {
            Ok(guard) => guard,
            Err(error) => {
                return Err(Box::new(HookModulePublicationFailure {
                    prepared: Self {
                        program,
                        state: error.prepared,
                    },
                    problem: HookModulePublicationProblem::Heap {
                        reason: error.reason,
                        cause: error.cause,
                    },
                }));
            }
        };
        match apply(&program) {
            Ok(receipt) => {
                guard.publish();
                Ok((program, receipt))
            }
            Err(error) => Err(Box::new(HookModulePublicationFailure {
                prepared: Self {
                    program,
                    state: guard.abort(),
                },
                problem: HookModulePublicationProblem::Renderer(error),
            })),
        }
    }
    pub fn changed(&self) -> bool {
        self.state.changed()
    }
    pub fn program(&self) -> &ModuleProgram {
        &self.program
    }
    pub fn state(&self) -> &PreparedDocument {
        &self.state
    }
}

#[derive(Debug)]
pub enum HookModulePublicationProblem<E> {
    Heap {
        reason: &'static str,
        cause: Option<crate::application::hook_values::Error>,
    },
    Renderer(E),
}
pub struct HookModulePublicationFailure<E> {
    pub prepared: PreparedHookModule,
    pub problem: HookModulePublicationProblem<E>,
}
impl<E: std::fmt::Debug> std::fmt::Debug for HookModulePublicationFailure<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookModulePublicationFailure")
            .field("problem", &self.problem)
            .finish_non_exhaustive()
    }
}

pub struct HookModuleFailure {
    pub rejected_actions: Vec<hook_document::BoundAction>,
    pub diagnostic: ModuleDiagnostic,
    pub state_failure: Option<Box<hook_document::Failure>>,
    candidate: Option<PreparedHookModule>,
}
impl std::fmt::Debug for HookModuleFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookModuleFailure")
            .field("diagnostic", &self.diagnostic)
            .finish_non_exhaustive()
    }
}
impl HookModuleFailure {
    pub fn candidate(&self) -> Option<&PreparedHookModule> {
        self.candidate.as_ref()
    }
    fn source(diagnostic: ModuleDiagnostic) -> Box<Self> {
        Box::new(Self {
            rejected_actions: Vec::new(),
            diagnostic,
            state_failure: None,
            candidate: None,
        })
    }
    fn state(file: &BuildPath, state_failure: Box<hook_document::Failure>) -> Box<Self> {
        Box::new(Self {
            rejected_actions: Vec::new(),
            diagnostic: ModuleDiagnostic {
                file: file.clone(),
                diagnostic: state_failure.diagnostic.clone(),
            },
            state_failure: Some(state_failure),
            candidate: None,
        })
    }
    fn projection(
        diagnostic: ModuleDiagnostic,
        program: ModuleProgram,
        mut state: PreparedDocument,
    ) -> Box<Self> {
        state.reject();
        Box::new(Self {
            rejected_actions: Vec::new(),
            diagnostic,
            state_failure: None,
            candidate: Some(PreparedHookModule { program, state }),
        })
    }
}

impl ModuleProgram {
    pub fn prepare_hook_actions(
        &self,
        hooks: &HookDocument,
        actions: Vec<hook_document::BoundAction>,
    ) -> Result<PreparedHookModule, Box<HookModuleFailure>> {
        if let Err(mut failure) = self.require_hook_baseline(hooks) {
            failure.rejected_actions = actions;
            return Err(failure);
        }
        let mut state = hooks.prepare_actions(actions).map_err(|error| {
            let file = error
                .instance()
                .and_then(|failed| {
                    self.mounted
                        .values()
                        .find_map(|mounted| match &mounted.state {
                            MountedState::Hook(handle) if handle == failed => {
                                Some(&mounted.reference.module)
                            }
                            _ => None,
                        })
                })
                .unwrap_or(&self.graph.entry.module);
            HookModuleFailure::state(file, error)
        })?;
        let mut program = self.clone();
        if state.changed()
            && let Err(error) = program.refresh_with_hooks(Some(&mut state))
        {
            return Err(HookModuleFailure::projection(error, program, state));
        }
        Ok(PreparedHookModule { program, state })
    }
    pub fn prepare_hook_mount(
        graph: SourceModuleGraph,
        props: BTreeMap<String, Value>,
        document: String,
        epoch: u64,
        hooks: &HookDocument,
    ) -> Result<PreparedHookModule, Box<HookModuleFailure>> {
        Self::prepare_hook_mount_with_node_slots(graph, document, epoch, hooks, |_| Ok(props))
    }

    pub fn prepare_hook_mount_with_node_slots(
        graph: SourceModuleGraph,
        document: String,
        epoch: u64,
        hooks: &HookDocument,
        prepare: impl FnOnce(&NodeSlotFactory<'_>) -> Result<BTreeMap<String, Value>, String>,
    ) -> Result<PreparedHookModule, Box<HookModuleFailure>> {
        let mut program =
            Self::configure(graph, document, epoch, prepare).map_err(HookModuleFailure::source)?;
        if hooks.stamp().revision != 0 {
            return Err(HookModuleFailure::source(failure(
                &program.graph.entry.module,
                Span::default(),
                "hook module mount requires a fresh document",
            )));
        }
        let registry_scope = ContentHash::of(
            &serde_json::to_vec(&(
                hooks.stamp().document,
                &program.document,
                program.epoch,
                program.slot_owner,
            ))
            .map_err(|error| {
                HookModuleFailure::source(failure(
                    &program.graph.entry.module,
                    Span::default(),
                    error.to_string(),
                ))
            })?,
        );
        program.hook_registry = Some(
            ComponentState::new(format!("hook-modules-{}", registry_scope.as_str()), epoch)
                .map_err(|message| {
                    HookModuleFailure::source(failure(
                        &program.graph.entry.module,
                        Span::default(),
                        message,
                    ))
                })?,
        );
        program.hook_stamp = Some(hooks.stamp());
        let mut state = hooks
            .prepare_actions(Vec::new())
            .map_err(|error| HookModuleFailure::state(&program.graph.entry.module, error))?;
        if let Err(error) = program.refresh_with_hooks(Some(&mut state)) {
            return Err(HookModuleFailure::projection(error, program, state));
        }
        Ok(PreparedHookModule { program, state })
    }

    fn require_hook_baseline(&self, hooks: &HookDocument) -> Result<(), Box<HookModuleFailure>> {
        if self.hook_stamp != Some(hooks.stamp()) || self.hook_registry.is_none() {
            return Err(HookModuleFailure::source(failure(
                &self.graph.entry.module,
                Span::default(),
                "stale or foreign hook module document baseline",
            )));
        }
        Ok(())
    }

    pub fn prepare_hook_action(
        &self,
        hooks: &HookDocument,
        path: &str,
    ) -> Result<PreparedHookModule, Box<HookModuleFailure>> {
        self.require_hook_baseline(hooks)?;
        let event = self
            .project()
            .map_err(HookModuleFailure::source)?
            .events
            .get(path)
            .ok_or_else(|| {
                HookModuleFailure::source(failure(
                    &self.graph.entry.module,
                    Span::default(),
                    "hook module event path missing",
                ))
            })?;
        let handler = event.handler.as_ref().ok_or_else(|| {
            HookModuleFailure::source(failure(
                &self.graph.entry.module,
                Span::default(),
                "module event has no hook frame handler",
            ))
        })?;
        let file = &self
            .mounted
            .get(&event.instance)
            .ok_or_else(|| {
                HookModuleFailure::source(failure(
                    &self.graph.entry.module,
                    Span::default(),
                    "hook module event instance missing",
                ))
            })?
            .reference
            .module;
        let mut state = hooks
            .prepare_actions(vec![handler.action()])
            .map_err(|error| HookModuleFailure::state(file, error))?;
        let mut program = self.clone();
        if state.changed()
            && let Err(error) = program.refresh_with_hooks(Some(&mut state))
        {
            return Err(HookModuleFailure::projection(error, program, state));
        }
        Ok(PreparedHookModule { program, state })
    }

    pub fn prepare_hook_props(
        &self,
        hooks: &HookDocument,
        props: BTreeMap<String, Value>,
    ) -> Result<PreparedHookModule, Box<HookModuleFailure>> {
        self.require_hook_baseline(hooks)?;
        let mut state = hooks
            .prepare_actions(Vec::new())
            .map_err(|error| HookModuleFailure::state(&self.graph.entry.module, error))?;
        let mut program = self.clone();
        if program.props != props {
            program.props = props;
            if let Err(diagnostic) = state.advance_frame() {
                return Err(HookModuleFailure::projection(
                    ModuleDiagnostic {
                        file: program.graph.entry.module.clone(),
                        diagnostic,
                    },
                    program,
                    state,
                ));
            }
            if let Err(error) = program.refresh_with_hooks(Some(&mut state)) {
                return Err(HookModuleFailure::projection(error, program, state));
            }
        }
        Ok(PreparedHookModule { program, state })
    }
}

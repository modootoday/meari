//! Split from the parent module to keep files under the 600-line structure limit.
use super::super::{
    hook_expression::{self, Bindings, EvaluationFailure},
    hook_values::{Budget, Error, Owner, Transaction, Value},
};
use super::*;
use crate::domain::{
    Diagnostic, Span,
    component_state::InstanceHandle,
    hook_expression::Expression,
    state_program::{StateEvent, hook_literal as literal},
};
use std::{cell::Cell, collections::BTreeMap, rc::Rc, sync::atomic::Ordering};

impl std::fmt::Debug for BoundHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoundHookHandler")
            .field("stamp", &self.stamp)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookDocumentFailure")
            .field("diagnostic", &self.diagnostic)
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}

pub(super) fn rejected(
    message: &str,
    span: Span,
    candidate: Option<Transaction>,
    actions: Vec<BoundAction>,
) -> Box<Failure> {
    Box::new(Failure {
        instance: None,
        diagnostic: Diagnostic::unsupported(span, message),
        cause: None,
        candidate,
        actions,
    })
}

pub(super) fn evaluate(
    candidate: &mut Option<Transaction>,
    expression: &Expression,
    bindings: Bindings<'_>,
) -> Result<Value, (Diagnostic, Option<Error>)> {
    match hook_expression::evaluate(
        candidate.take().expect("owned transaction"),
        expression,
        bindings,
    ) {
        Ok(evaluated) => {
            *candidate = Some(evaluated.candidate);
            Ok(evaluated.root)
        }
        Err(failure) => {
            let EvaluationFailure {
                candidate: original,
                diagnostic,
                cause,
            } = *failure;
            *candidate = Some(original);
            Err((diagnostic, cause))
        }
    }
}

pub(super) fn bind(
    stamp: DocumentStamp,
    instances: &Instances,
    epoch: &Rc<Epoch>,
    instance: &InstanceHandle,
    event: StateEvent,
) -> Result<BoundAction, Diagnostic> {
    let state = instances
        .get(instance)
        .ok_or_else(|| Diagnostic::unsupported(Span::default(), "hook instance is not mounted"))?;
    event
        .validate(&state.bindings)
        .map_err(|message| Diagnostic::unsupported(Span::default(), message))?;
    Ok(BoundAction {
        stamp,
        instance: instance.clone(),
        definition_revision: state.definition_revision,
        event,
        captured: state.roots.clone(),
        epoch: Rc::downgrade(epoch),
    })
}

impl HookDocument {
    pub fn new(source_generation: u64, budget: Budget) -> Result<Self, &'static str> {
        if source_generation == 0 {
            return Err("positive source generation required");
        }
        let id = NEXT_DOCUMENT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| "hook document identity exhausted")?;
        Ok(Self {
            id,
            source_generation,
            revision: 0,
            owner: Owner::new(budget),
            instances: BTreeMap::new(),
            epoch: Rc::new(Epoch {
                active: Cell::new(true),
            }),
            closed: false,
        })
    }

    pub fn stamp(&self) -> DocumentStamp {
        DocumentStamp {
            document: self.id,
            source_generation: self.source_generation,
            revision: self.revision,
        }
    }

    pub fn values(&self) -> &Owner {
        &self.owner
    }

    pub fn snapshot(&self, instance: &InstanceHandle) -> Result<Snapshot, Diagnostic> {
        if self.closed {
            return Err(Diagnostic::unsupported(
                Span::default(),
                "hook document is closed",
            ));
        }
        let state = self.instances.get(instance).ok_or_else(|| {
            Diagnostic::unsupported(Span::default(), "hook instance is not mounted")
        })?;
        Ok(Snapshot {
            stamp: self.stamp(),
            definition_revision: state.definition_revision,
            roots: state.roots.clone(),
        })
    }

    pub fn capture(
        &self,
        instance: &InstanceHandle,
        event: StateEvent,
    ) -> Result<BoundAction, Diagnostic> {
        if self.closed {
            return Err(Diagnostic::unsupported(
                Span::default(),
                "hook document is closed",
            ));
        }
        bind(self.stamp(), &self.instances, &self.epoch, instance, event)
    }

    pub(super) fn begin(&self) -> Result<Option<Transaction>, Box<Failure>> {
        self.owner.begin().map(Some).map_err(|cause| {
            Box::new(Failure {
                instance: None,
                diagnostic: Diagnostic::unsupported(Span::default(), "hook candidate cannot begin"),
                cause: Some(cause),
                candidate: None,
                actions: Vec::new(),
            })
        })
    }

    pub fn prepare_mounts(&self, mounts: Vec<Mount>) -> Result<PreparedDocument, Box<Failure>> {
        let mut candidate = self.begin()?;
        let mut instances = self.instances.clone();
        for mount in mounts {
            if mount.definition_revision == 0
                || instances.contains_key(&mount.instance)
                || instances.len() >= 16_384
            {
                return Err(rejected(
                    "invalid/duplicate hook mount or instance budget exceeded",
                    mount.definition.root.span,
                    candidate,
                    Vec::new(),
                ));
            }
            if let Err(diagnostic) = crate::domain::validation::validate(&mount.definition) {
                return Err(Box::new(Failure {
                    instance: Some(mount.instance.clone()),
                    diagnostic,
                    cause: None,
                    candidate,
                    actions: Vec::new(),
                }));
            }
            let mut roots = Roots::new();
            for binding in &mount.definition.states {
                let expression = match literal(&binding.initial, binding.span) {
                    Ok(value) => value,
                    Err(diagnostic) => {
                        return Err(Box::new(Failure {
                            instance: Some(mount.instance.clone()),
                            diagnostic,
                            cause: None,
                            candidate,
                            actions: Vec::new(),
                        }));
                    }
                };
                let value = match evaluate(
                    &mut candidate,
                    &expression,
                    Bindings {
                        states: &roots,
                        previous: None,
                    },
                ) {
                    Ok(value) => value,
                    Err((diagnostic, cause)) => {
                        return Err(Box::new(Failure {
                            instance: Some(mount.instance.clone()),
                            diagnostic,
                            cause,
                            candidate,
                            actions: Vec::new(),
                        }));
                    }
                };
                roots.insert(binding.name.clone(), value);
            }
            instances.insert(
                mount.instance,
                Instance {
                    bindings: mount.definition.states,
                    definition_revision: mount.definition_revision,
                    roots,
                },
            );
        }
        let changed = instances.len() != self.instances.len();
        self.finish(candidate, instances, changed, Vec::new())
    }

    pub fn prepare_actions(
        &self,
        actions: Vec<BoundAction>,
    ) -> Result<PreparedDocument, Box<Failure>> {
        let mut candidate = match self.begin() {
            Ok(candidate) => candidate,
            Err(mut failure) => {
                failure.actions = actions;
                return Err(failure);
            }
        };
        let mut instances = self.instances.clone();
        for action in &actions {
            let action_instance = action.instance.clone();
            if action.stamp != self.stamp()
                || !action
                    .epoch
                    .upgrade()
                    .is_some_and(|epoch| epoch.active.get() && Rc::ptr_eq(&epoch, &self.epoch))
            {
                return Err(rejected(
                    "stale or foreign hook action scope",
                    Span::default(),
                    candidate,
                    actions,
                )
                .for_instance(action_instance));
            }
            let Some(instance) = instances
                .get_mut(&action.instance)
                .filter(|instance| instance.definition_revision == action.definition_revision)
            else {
                return Err(rejected(
                    "hook instance/definition was replaced",
                    Span::default(),
                    candidate,
                    actions,
                )
                .for_instance(action_instance));
            };
            let Some(binding) = instance
                .bindings
                .iter()
                .find(|binding| binding.setter == action.event.setter)
            else {
                return Err(rejected(
                    "hook setter disappeared",
                    Span::default(),
                    candidate,
                    actions,
                )
                .for_instance(action_instance));
            };
            let expression = match action.event.hook_expression(binding.span) {
                Ok(value) => value,
                Err(diagnostic) => {
                    return Err(Box::new(Failure {
                        instance: Some(action_instance),
                        diagnostic,
                        cause: None,
                        candidate,
                        actions,
                    }));
                }
            };
            let previous = instance.roots.get(&binding.name);
            let root = match evaluate(
                &mut candidate,
                &expression,
                Bindings {
                    states: &action.captured,
                    previous,
                },
            ) {
                Ok(value) => value,
                Err((diagnostic, cause)) => {
                    return Err(Box::new(Failure {
                        instance: Some(action_instance),
                        diagnostic,
                        cause,
                        candidate,
                        actions,
                    }));
                }
            };
            let expected = match crate::domain::hook_expression::shape::state(&binding.initial) {
                Ok(expected) => expected,
                Err(message) => {
                    return Err(rejected(&message, binding.span, candidate, actions)
                        .for_instance(action_instance));
                }
            };
            match shape::accepts(
                candidate.as_ref().expect("owned transaction"),
                &root,
                &expected,
            ) {
                Ok(true) => (),
                result => {
                    candidate
                        .as_mut()
                        .expect("owned transaction")
                        .reject_expression();
                    return Err(Box::new(Failure {
                        instance: Some(action_instance),
                        diagnostic: Diagnostic::unsupported(
                            expression.span,
                            "hook update result does not satisfy the declared state shape",
                        ),
                        cause: result.err(),
                        candidate,
                        actions,
                    }));
                }
            }
            instance.roots.insert(binding.name.clone(), root);
        }
        let tx = candidate.as_ref().expect("owned transaction");
        let mut changed = false;
        for (key, instance) in &instances {
            for (slot, value) in &instance.roots {
                let old = &self.instances[key].roots[slot];
                match tx.same_value(old, value) {
                    Ok(equal) => changed |= !equal,
                    Err(cause) => {
                        return Err(Box::new(Failure {
                            instance: Some(key.clone()),
                            diagnostic: Diagnostic::unsupported(
                                Span::default(),
                                "hook root comparison failed",
                            ),
                            cause: Some(cause),
                            candidate,
                            actions,
                        }));
                    }
                }
            }
        }
        self.finish(candidate, instances, changed, actions)
    }

    pub(super) fn finish(
        &self,
        mut candidate: Option<Transaction>,
        mut instances: Instances,
        changed: bool,
        actions: Vec<BoundAction>,
    ) -> Result<PreparedDocument, Box<Failure>> {
        let revision = if changed {
            match self.revision.checked_add(1) {
                Some(value) => value,
                None => {
                    return Err(rejected(
                        "hook document revision exhausted",
                        Span::default(),
                        candidate,
                        actions,
                    ));
                }
            }
        } else {
            self.revision
        };
        if !changed {
            drop(candidate.take());
            candidate = match self.begin() {
                Ok(candidate) => candidate,
                Err(mut failure) => {
                    failure.actions = actions;
                    return Err(failure);
                }
            };
            instances = self.instances.clone();
        }
        Ok(PreparedDocument {
            baseline: self.stamp(),
            next: DocumentStamp {
                revision,
                ..self.stamp()
            },
            candidate,
            instances,
            epoch: if changed {
                Rc::new(Epoch {
                    active: Cell::new(false),
                })
            } else {
                self.epoch.clone()
            },
            changed,
        })
    }
}

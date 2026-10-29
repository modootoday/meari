//! One document heap and an atomic instance-root publication. No renderer is invoked here.
use super::{
    hook_expression::{self, Bindings, EvaluationFailure},
    hook_values::{Budget, Error, Owner, PreparedPublish, Transaction, Value},
};
use crate::domain::{
    Component, Diagnostic, Span,
    component_state::InstanceHandle,
    hook_expression::Expression,
    state_program::{StateBinding, StateEvent, hook_literal as literal},
};
use std::{
    cell::Cell,
    collections::BTreeMap,
    rc::{Rc, Weak},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT_DOCUMENT: AtomicU64 = AtomicU64::new(1);
mod projection;
mod shape;
type Roots = BTreeMap<String, Value>;
type Instances = BTreeMap<InstanceHandle, Instance>;
#[derive(Clone)]
struct Instance {
    bindings: Vec<StateBinding>,
    definition_revision: u64,
    roots: Roots,
}
struct Epoch {
    active: Cell<bool>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocumentStamp {
    pub document: u64,
    pub source_generation: u64,
    pub revision: u64,
}
pub struct Mount {
    pub instance: InstanceHandle,
    pub definition: Component,
    pub definition_revision: u64,
}
pub struct Snapshot {
    pub stamp: DocumentStamp,
    pub definition_revision: u64,
    pub roots: Roots,
}
pub struct BoundAction {
    stamp: DocumentStamp,
    instance: InstanceHandle,
    definition_revision: u64,
    event: StateEvent,
    captured: Roots,
    epoch: Weak<Epoch>,
}
pub struct Failure {
    instance: Option<InstanceHandle>,
    pub diagnostic: Diagnostic,
    pub cause: Option<Error>,
    pub candidate: Option<Transaction>,
    pub actions: Vec<BoundAction>,
}
impl Failure {
    pub fn instance(&self) -> Option<&InstanceHandle> {
        self.instance.as_ref()
    }
    fn for_instance(mut self: Box<Self>, instance: InstanceHandle) -> Box<Self> {
        self.instance = Some(instance);
        self
    }
}
/// Cloneable frame handler; activation still produces a move-only, scope-checked action.
#[derive(Clone)]
pub struct BoundHandler {
    stamp: DocumentStamp,
    instance: InstanceHandle,
    definition_revision: u64,
    event: StateEvent,
    captured: Roots,
    epoch: Weak<Epoch>,
}
impl BoundHandler {
    pub fn action(&self) -> BoundAction {
        BoundAction {
            stamp: self.stamp,
            instance: self.instance.clone(),
            definition_revision: self.definition_revision,
            event: self.event.clone(),
            captured: self.captured.clone(),
            epoch: self.epoch.clone(),
        }
    }
}
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
fn rejected(
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
fn evaluate(
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
fn bind(
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

pub struct HookDocument {
    id: u64,
    source_generation: u64,
    revision: u64,
    owner: Owner,
    instances: Instances,
    epoch: Rc<Epoch>,
    closed: bool,
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
    fn begin(&self) -> Result<Option<Transaction>, Box<Failure>> {
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
    fn finish(
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
    /// Hold this guard across the independent renderer apply and all receipt validation.
    pub fn prepare_publication(
        &mut self,
        prepared: PreparedDocument,
    ) -> Result<Publication<'_>, Box<PublicationFailure>> {
        if self.closed || prepared.baseline != self.stamp() {
            return Err(Box::new(PublicationFailure {
                prepared,
                cause: None,
                reason: if self.closed {
                    "hook document is closed"
                } else {
                    "stale or foreign hook publication"
                },
            }));
        }
        let PreparedDocument {
            baseline,
            next,
            candidate,
            instances,
            epoch,
            changed,
        } = prepared;
        let heap = match self
            .owner
            .prepare_publish(candidate.expect("owned transaction"))
        {
            Ok(heap) => heap,
            Err(failure) => {
                return Err(Box::new(PublicationFailure {
                    cause: Some(failure.error),
                    reason: "hook heap rejected publication",
                    prepared: PreparedDocument {
                        baseline,
                        next,
                        candidate: Some(failure.candidate),
                        instances,
                        epoch,
                        changed,
                    },
                }));
            }
        };
        Ok(Publication {
            heap,
            instances_target: &mut self.instances,
            revision_target: &mut self.revision,
            epoch_target: &mut self.epoch,
            baseline,
            next,
            instances,
            epoch,
            changed,
        })
    }
    pub fn close(&mut self) {
        self.closed = true;
        self.epoch.active.set(false);
        self.instances.clear();
        self.owner.close();
    }
}
impl Drop for HookDocument {
    fn drop(&mut self) {
        self.close();
    }
}

pub struct PreparedDocument {
    baseline: DocumentStamp,
    next: DocumentStamp,
    candidate: Option<Transaction>,
    instances: Instances,
    epoch: Rc<Epoch>,
    changed: bool,
}
impl PreparedDocument {
    pub(crate) fn reject(&mut self) {
        self.candidate
            .as_mut()
            .expect("owned transaction")
            .reject_expression();
    }
    pub(crate) fn stamp(&self) -> DocumentStamp {
        self.next
    }

    pub(crate) fn advance_frame(&mut self) -> Result<(), Diagnostic> {
        if !self.changed {
            self.next.revision = self.baseline.revision.checked_add(1).ok_or_else(|| {
                Diagnostic::unsupported(Span::default(), "hook document revision exhausted")
            })?;
            self.epoch = Rc::new(Epoch {
                active: Cell::new(false),
            });
            self.changed = true;
        }
        Ok(())
    }

    pub(crate) fn ensure_instance(&mut self, mount: Mount) -> Result<(), Diagnostic> {
        crate::domain::validation::validate(&mount.definition)?;
        if mount.definition_revision == 0 {
            return Err(Diagnostic::unsupported(
                mount.definition.root.span,
                "positive hook definition revision required",
            ));
        }
        if let Some(current) = self.instances.get(&mount.instance) {
            if current.definition_revision != mount.definition_revision
                || current.bindings != mount.definition.states
            {
                return Err(Diagnostic::unsupported(
                    mount.definition.root.span,
                    "hook definition replacement requires a prepared migration",
                ));
            }
            return Ok(());
        }
        self.advance_frame()?;
        if self.instances.len() >= 16_384 {
            self.reject();
            return Err(Diagnostic::unsupported(
                mount.definition.root.span,
                "hook instance budget exceeded",
            ));
        }
        let mut roots = Roots::new();
        for binding in &mount.definition.states {
            let expression = literal(&binding.initial, binding.span)?;
            let root = evaluate(
                &mut self.candidate,
                &expression,
                Bindings {
                    states: &roots,
                    previous: None,
                },
            )
            .map_err(|(diagnostic, _)| diagnostic)?;
            roots.insert(binding.name.clone(), root);
        }
        self.instances.insert(
            mount.instance,
            Instance {
                bindings: mount.definition.states,
                definition_revision: mount.definition_revision,
                roots,
            },
        );
        Ok(())
    }

    pub(crate) fn retain_instances(
        &mut self,
        active: &std::collections::BTreeSet<InstanceHandle>,
    ) -> Result<(), Diagnostic> {
        if self
            .instances
            .keys()
            .any(|instance| !active.contains(instance))
        {
            self.advance_frame()?;
            self.instances
                .retain(|instance, _| active.contains(instance));
        }
        Ok(())
    }

    pub(crate) fn handler(
        &self,
        instance: &InstanceHandle,
        event: StateEvent,
    ) -> Result<BoundHandler, Diagnostic> {
        let BoundAction {
            stamp,
            instance,
            definition_revision,
            event,
            captured,
            epoch,
        } = self.capture(instance, event)?;
        Ok(BoundHandler {
            stamp,
            instance,
            definition_revision,
            event,
            captured,
            epoch,
        })
    }

    pub fn changed(&self) -> bool {
        self.changed
    }
    pub fn values(&self) -> &Transaction {
        self.candidate.as_ref().expect("owned transaction")
    }
    pub fn root(&self, instance: &InstanceHandle, slot: &str) -> Option<&Value> {
        self.instances.get(instance)?.roots.get(slot)
    }
    pub fn capture(
        &self,
        instance: &InstanceHandle,
        event: StateEvent,
    ) -> Result<BoundAction, Diagnostic> {
        if !self.changed {
            return Err(Diagnostic::unsupported(
                Span::default(),
                "unchanged candidate cannot issue a new frame action",
            ));
        }
        bind(self.next, &self.instances, &self.epoch, instance, event)
    }
}
pub struct PublicationFailure {
    pub prepared: PreparedDocument,
    pub cause: Option<Error>,
    pub reason: &'static str,
}
impl std::fmt::Debug for PublicationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookPublicationFailure")
            .field("reason", &self.reason)
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
pub struct Publication<'a> {
    heap: PreparedPublish<'a>,
    instances_target: &'a mut Instances,
    revision_target: &'a mut u64,
    epoch_target: &'a mut Rc<Epoch>,
    baseline: DocumentStamp,
    next: DocumentStamp,
    instances: Instances,
    epoch: Rc<Epoch>,
    changed: bool,
}
impl Publication<'_> {
    /// Heap/root publication only. This never fabricates a renderer success receipt.
    pub fn publish(self) -> DocumentStamp {
        self.heap.publish();
        *self.instances_target = self.instances;
        if self.changed {
            self.epoch_target.active.set(false);
            self.epoch.active.set(true);
        }
        *self.epoch_target = self.epoch;
        *self.revision_target = self.next.revision;
        self.next
    }
    pub fn abort(self) -> PreparedDocument {
        PreparedDocument {
            baseline: self.baseline,
            next: self.next,
            candidate: Some(self.heap.abort()),
            instances: self.instances,
            epoch: self.epoch,
            changed: self.changed,
        }
    }
}

#[cfg(test)]
mod tests;

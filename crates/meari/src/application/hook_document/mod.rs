//! One document heap and an atomic instance-root publication. No renderer is invoked here.
use super::hook_values::{Error, Owner, PreparedPublish, Transaction, Value};
use crate::domain::{
    Component, Diagnostic,
    component_state::InstanceHandle,
    state_program::{StateBinding, StateEvent},
};
use std::{
    cell::Cell,
    collections::BTreeMap,
    rc::{Rc, Weak},
    sync::atomic::AtomicU64,
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

pub struct HookDocument {
    id: u64,
    source_generation: u64,
    revision: u64,
    owner: Owner,
    instances: Instances,
    epoch: Rc<Epoch>,
    closed: bool,
}

pub struct PreparedDocument {
    baseline: DocumentStamp,
    next: DocumentStamp,
    candidate: Option<Transaction>,
    instances: Instances,
    epoch: Rc<Epoch>,
    changed: bool,
}
pub struct PublicationFailure {
    pub prepared: PreparedDocument,
    pub cause: Option<Error>,
    pub reason: &'static str,
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

mod evaluation;
use self::evaluation::*;
mod publication;

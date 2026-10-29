//! Bounded Rust value ownership, not a React hook frontend or serialized dispatch authority.
//!
//! Values are immutable, acyclic plain records/dense arrays and scalar values. Functions,
//! symbols, BigInt, prototypes, host objects and UTF-16 lone surrogates are unsupported.
//! An owner retains committed nodes until close. Transactions share immutable committed
//! nodes, stage new nodes, and never rewind identity allocation on rollback.
use crate::domain::hook_value::Node;
pub use crate::domain::hook_value::ValueKind;
use std::{
    cell::Cell,
    collections::BTreeMap,
    rc::{Rc, Weak},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidExpression,
    Busy,
    Closed,
    ForeignOwner,
    StaleValue,
    StaleTransaction,
    BudgetExceeded,
    Overflow,
    DuplicateField,
    WrongKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub nodes: usize,
    pub edges: usize,
    /// Retained String buffer capacities, including record keys; not total allocator/RSS bytes.
    pub bytes: usize,
}
impl Usage {
    fn add(self, other: Self) -> Result<Self, Error> {
        Ok(Self {
            nodes: self.nodes.checked_add(other.nodes).ok_or(Error::Overflow)?,
            edges: self.edges.checked_add(other.edges).ok_or(Error::Overflow)?,
            bytes: self.bytes.checked_add(other.bytes).ok_or(Error::Overflow)?,
        })
    }
    fn subtract(self, other: Self) -> Self {
        Self {
            nodes: self.nodes - other.nodes,
            edges: self.edges - other.edges,
            bytes: self.bytes - other.bytes,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Budget {
    max_nodes: usize,
    max_edges: usize,
    max_bytes: usize,
    max_depth: usize,
}
impl Budget {
    pub fn new(nodes: usize, edges: usize, bytes: usize, depth: usize) -> Result<Self, Error> {
        if nodes == 0 || edges == 0 || bytes == 0 || depth == 0 {
            return Err(Error::BudgetExceeded);
        }
        Ok(Self {
            max_nodes: nodes,
            max_edges: edges,
            max_bytes: bytes,
            max_depth: depth,
        })
    }
    fn check(self, usage: Usage, depth: usize) -> Result<(), Error> {
        if usage.nodes > self.max_nodes
            || usage.edges > self.max_edges
            || usage.bytes > self.max_bytes
            || depth > self.max_depth
        {
            Err(Error::BudgetExceeded)
        } else {
            Ok(())
        }
    }
}

struct Identity {
    active: Cell<bool>,
    publishing: Cell<bool>,
    sequence: Cell<u64>,
    revision: Cell<u64>,
    committed: Cell<Usage>,
    reserved: Cell<Usage>,
    candidates: Cell<usize>,
}

/// Clone preserves alias identity, never a new allocation. No public fields or serde.
#[derive(Clone)]
pub struct Value {
    owner: Weak<Identity>,
    id: u64,
}
type Nodes = BTreeMap<u64, Rc<Node>>;

/// One live runtime scope. Committed nodes are reclaimed on close/Drop, not per handle Drop.
/// At most 64 transaction candidates may be outstanding. No instance/HMR binding is implied.
pub struct Owner {
    identity: Rc<Identity>,
    budget: Budget,
    nodes: Rc<Nodes>,
}

/// Original rejected candidate, retained for caller-owned diagnostics or explicit disposal.
pub struct PreparePublishFailure {
    pub error: Error,
    pub candidate: Transaction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeapPublication {
    pub usage: Usage,
    pub revision: u64,
    pub changed: bool,
}

/// Exclusive, move-only publication preparation. No renderer or callback is invoked here.
/// The final publish performs no allocation, parsing, OS calls or fallible validation.
/// Other escaped candidates may drop while held, but cannot allocate, read or publish.
/// The original owner cannot be closed or otherwise mutably reused before publication ends.
/// ```compile_fail,E0499
/// use meari::application::hook_values::{Budget, Owner};
/// let mut owner = Owner::new(Budget::default());
/// let candidate = owner.begin().unwrap();
/// let prepared = owner.prepare_publish(candidate).unwrap();
/// owner.close();
/// prepared.publish();
/// ```
pub struct PreparedPublish<'owner> {
    owner: &'owner mut Owner,
    candidate: Option<Transaction>,
    nodes: Rc<Nodes>,
    publication: HeapPublication,
}
impl PreparedPublish<'_> {
    /// Return the same uncommitted candidate and release the publication barrier.
    pub fn abort(mut self) -> Transaction {
        self.candidate.take().expect("owned candidate")
    }
    /// Call only after the independent renderer receipt has been fully validated.
    /// This receipt describes the heap alone, never renderer/DOM success.
    pub fn publish(mut self) -> HeapPublication {
        let mut candidate = self.candidate.take().expect("owned candidate");
        self.owner.nodes = self.nodes.clone();
        self.owner.identity.committed.set(self.publication.usage);
        candidate.release_reservation();
        self.owner.identity.revision.set(self.publication.revision);
        self.publication
    }
}

/// Staged allocation is charged across all outstanding candidates of the same owner.
/// Drop rolls back staged nodes; escaped staged handles cannot be read by the owner.
/// A failed allocation poisons the candidate: commit returns an error and rolls it back.
pub struct Transaction {
    identity: Rc<Identity>,
    baseline: u64,
    budget: Budget,
    base: Rc<Nodes>,
    staged: Nodes,
    reserved: Usage,
    failure: Option<Error>,
}

#[cfg(test)]
mod tests;

mod costs;
mod validation;

//! Bounded Rust value ownership, not a React hook frontend or serialized dispatch authority.
//!
//! Values are immutable, acyclic plain records/dense arrays and scalar values. Functions,
//! symbols, BigInt, prototypes, host objects and UTF-16 lone surrogates are unsupported.
//! An owner retains committed nodes until close. Transactions share immutable committed
//! nodes, stage new nodes, and never rewind identity allocation on rollback.
pub use crate::domain::hook_value::ValueKind;
use crate::domain::hook_value::{Data, Node, same_value};
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
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "hook value: {self:?}")
    }
}
impl std::error::Error for Error {}

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
impl Default for Budget {
    fn default() -> Self {
        Self {
            max_nodes: 16_384,
            max_edges: 16_384,
            max_bytes: 1024 * 1024,
            max_depth: 32,
        }
    }
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
impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HookValue(..)")
    }
}
type Nodes = BTreeMap<u64, Rc<Node>>;

fn validate(identity: &Rc<Identity>, value: &Value) -> Result<(), Error> {
    if !identity.active.get() {
        return Err(Error::Closed);
    }
    if !Weak::ptr_eq(&Rc::downgrade(identity), &value.owner) {
        return Err(Error::ForeignOwner);
    }
    Ok(())
}
fn value(identity: &Rc<Identity>, id: u64) -> Value {
    Value {
        owner: Rc::downgrade(identity),
        id,
    }
}

fn read_field(identity: &Rc<Identity>, node: &Node, key: &str) -> Result<Option<Value>, Error> {
    let Data::Record(fields) = &node.data else {
        return Err(Error::WrongKind);
    };
    Ok(fields.get(key).map(|id| value(identity, *id)))
}
fn read_element(
    identity: &Rc<Identity>,
    node: &Node,
    index: usize,
) -> Result<Option<Value>, Error> {
    let Data::Array(items) = &node.data else {
        return Err(Error::WrongKind);
    };
    Ok(items.get(index).map(|id| value(identity, *id)))
}
fn read_number(node: &Node) -> Result<f64, Error> {
    let Data::Number(number) = node.data else {
        return Err(Error::WrongKind);
    };
    Ok(number)
}
fn read_boolean(node: &Node) -> Result<bool, Error> {
    let Data::Boolean(boolean) = node.data else {
        return Err(Error::WrongKind);
    };
    Ok(boolean)
}
fn read_text(node: &Node) -> Result<&str, Error> {
    let Data::String(text) = &node.data else {
        return Err(Error::WrongKind);
    };
    Ok(text)
}

/// One live runtime scope. Committed nodes are reclaimed on close/Drop, not per handle Drop.
/// At most 64 transaction candidates may be outstanding. No instance/HMR binding is implied.
pub struct Owner {
    identity: Rc<Identity>,
    budget: Budget,
    nodes: Rc<Nodes>,
}
impl Owner {
    pub fn new(budget: Budget) -> Self {
        Self {
            identity: Rc::new(Identity {
                active: Cell::new(true),
                publishing: Cell::new(false),
                sequence: Cell::new(0),
                revision: Cell::new(0),
                committed: Cell::new(Usage::default()),
                reserved: Cell::new(Usage::default()),
                candidates: Cell::new(0),
            }),
            budget,
            nodes: Rc::new(BTreeMap::new()),
        }
    }
    pub fn usage(&self) -> Usage {
        self.identity.committed.get()
    }
    pub fn begin(&self) -> Result<Transaction, Error> {
        if !self.identity.active.get() {
            return Err(Error::Closed);
        }
        if self.identity.publishing.get() {
            return Err(Error::Busy);
        }
        let candidates = self
            .identity
            .candidates
            .get()
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        if candidates > 64 {
            return Err(Error::BudgetExceeded);
        }
        self.identity.candidates.set(candidates);
        Ok(Transaction {
            identity: self.identity.clone(),
            baseline: self.identity.revision.get(),
            budget: self.budget,
            base: self.nodes.clone(),
            staged: BTreeMap::new(),
            reserved: Usage::default(),
            failure: None,
        })
    }
    /// An error consumes/rolls back the candidate; original committed values stay unchanged.
    pub fn commit(&mut self, mut transaction: Transaction) -> Result<(), Error> {
        if !Rc::ptr_eq(&self.identity, &transaction.identity) {
            return Err(Error::ForeignOwner);
        }
        transaction.current()?;
        if transaction.staged.is_empty() {
            return Ok(());
        }
        let revision = self
            .identity
            .revision
            .get()
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        let usage = self.usage().add(transaction.reserved)?;
        Rc::make_mut(&mut self.nodes).append(&mut transaction.staged);
        self.identity.committed.set(usage);
        transaction.release_reservation();
        self.identity.revision.set(revision);
        Ok(())
    }
    /// Prepare heap publication before applying a separately owned renderer candidate.
    /// Failure returns the original transaction. Dropping the guard aborts the candidate;
    /// it does not undo renderer effects. This prepares one owner, not a multi-owner batch.
    pub fn prepare_publish(
        &mut self,
        transaction: Transaction,
    ) -> Result<PreparedPublish<'_>, PreparePublishFailure> {
        let checked = (|| {
            if !Rc::ptr_eq(&self.identity, &transaction.identity) {
                return Err(Error::ForeignOwner);
            }
            transaction.current()?;
            let changed = !transaction.staged.is_empty();
            let revision = if changed {
                self.identity
                    .revision
                    .get()
                    .checked_add(1)
                    .ok_or(Error::Overflow)?
            } else {
                self.identity.revision.get()
            };
            Ok((self.usage().add(transaction.reserved)?, revision, changed))
        })();
        let (usage, revision, changed) = match checked {
            Ok(prepared) => prepared,
            Err(error) => {
                return Err(PreparePublishFailure {
                    error,
                    candidate: transaction,
                });
            }
        };
        let nodes = self.nodes.clone();
        self.identity.publishing.set(true);
        // Install RAII before prebuilding maps: unwind must release the barrier.
        let mut guard = PreparedPublish {
            owner: self,
            candidate: Some(transaction),
            nodes,
            publication: HeapPublication {
                usage,
                revision,
                changed,
            },
        };
        if changed {
            let mut nodes = guard.nodes.as_ref().clone();
            nodes.extend(
                guard
                    .candidate
                    .as_ref()
                    .expect("owned candidate")
                    .staged
                    .iter()
                    .map(|(id, node)| (*id, node.clone())),
            );
            guard.nodes = Rc::new(nodes);
        }
        Ok(guard)
    }
    fn node(&self, value: &Value) -> Result<&Node, Error> {
        validate(&self.identity, value)?;
        self.nodes
            .get(&value.id)
            .map(Rc::as_ref)
            .ok_or(Error::StaleValue)
    }
    pub fn kind(&self, value: &Value) -> Result<ValueKind, Error> {
        Ok(self.node(value)?.data.kind())
    }
    pub fn same_value(&self, left: &Value, right: &Value) -> Result<bool, Error> {
        Ok(same_value(
            &self.node(left)?.data,
            &self.node(right)?.data,
            left.id == right.id,
        ))
    }
    pub fn field(&self, record: &Value, key: &str) -> Result<Option<Value>, Error> {
        read_field(&self.identity, self.node(record)?, key)
    }
    pub fn element(&self, array: &Value, index: usize) -> Result<Option<Value>, Error> {
        read_element(&self.identity, self.node(array)?, index)
    }
    pub fn number(&self, value: &Value) -> Result<f64, Error> {
        read_number(self.node(value)?)
    }
    pub fn boolean(&self, value: &Value) -> Result<bool, Error> {
        read_boolean(self.node(value)?)
    }
    pub fn text<'a>(&'a self, value: &Value) -> Result<&'a str, Error> {
        read_text(self.node(value)?)
    }
    pub fn close(&mut self) {
        self.identity.active.set(false);
        self.nodes = Rc::new(BTreeMap::new());
        self.identity.committed.set(Usage::default());
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.close();
    }
}

/// Original rejected candidate, retained for caller-owned diagnostics or explicit disposal.
pub struct PreparePublishFailure {
    pub error: Error,
    pub candidate: Transaction,
}
impl std::fmt::Debug for PreparePublishFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparePublishFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
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
impl Drop for PreparedPublish<'_> {
    fn drop(&mut self) {
        // Candidate Drop releases only its own reservation; peer Drop is independently safe.
        drop(self.candidate.take());
        self.owner.identity.publishing.set(false);
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
impl Transaction {
    pub(crate) fn reject_expression(&mut self) {
        self.failure.get_or_insert(Error::InvalidExpression);
    }
    /// Read immutable own data fields. Ordering is canonical map order, not JS enumeration order.
    pub fn fields<'a>(
        &'a self,
        record: &Value,
    ) -> Result<impl Iterator<Item = (&'a str, Value)> + 'a, Error> {
        let Data::Record(fields) = &self.node(record)?.data else {
            return Err(Error::WrongKind);
        };
        Ok(fields
            .iter()
            .map(move |(key, id)| (key.as_str(), value(&self.identity, *id))))
    }
    /// Reads the current immutable candidate without publishing it to the owner.
    pub fn kind(&self, value: &Value) -> Result<ValueKind, Error> {
        Ok(self.node(value)?.data.kind())
    }
    pub fn field(&self, record: &Value, key: &str) -> Result<Option<Value>, Error> {
        read_field(&self.identity, self.node(record)?, key)
    }
    pub fn element(&self, array: &Value, index: usize) -> Result<Option<Value>, Error> {
        read_element(&self.identity, self.node(array)?, index)
    }
    pub fn read_number(&self, value: &Value) -> Result<f64, Error> {
        read_number(self.node(value)?)
    }
    pub fn read_boolean(&self, value: &Value) -> Result<bool, Error> {
        read_boolean(self.node(value)?)
    }
    /// The text borrow ends before the transaction can be mutated, committed or dropped.
    /// ```compile_fail
    /// use meari::application::hook_values::{Budget, Owner};
    /// let owner = Owner::new(Budget::default());
    /// let mut candidate = owner.begin().unwrap();
    /// let value = candidate.string("held".into()).unwrap();
    /// let text = candidate.read_text(&value).unwrap();
    /// candidate.null().unwrap();
    /// println!("{text}");
    /// ```
    pub fn read_text<'a>(&'a self, value: &Value) -> Result<&'a str, Error> {
        read_text(self.node(value)?)
    }
    fn current(&self) -> Result<(), Error> {
        if !self.identity.active.get() {
            return Err(Error::Closed);
        }
        if self.identity.publishing.get() {
            return Err(Error::Busy);
        }
        if self.identity.revision.get() != self.baseline {
            return Err(Error::StaleTransaction);
        }
        if let Some(error) = self.failure {
            return Err(error);
        }
        Ok(())
    }
    fn node(&self, value: &Value) -> Result<&Node, Error> {
        self.current()?;
        validate(&self.identity, value)?;
        self.staged
            .get(&value.id)
            .or_else(|| self.base.get(&value.id))
            .map(Rc::as_ref)
            .ok_or(Error::StaleValue)
    }
    fn allocate(&mut self, data: Data, depth: usize, cost: Usage) -> Result<Value, Error> {
        let outcome = self.allocate_inner(data, depth, cost);
        self.finish(outcome)
    }
    fn finish<T>(&mut self, outcome: Result<T, Error>) -> Result<T, Error> {
        if let Err(error) = &outcome
            && *error != Error::Busy
        {
            self.failure.get_or_insert(*error);
        }
        outcome
    }
    fn allocate_inner(&mut self, data: Data, depth: usize, cost: Usage) -> Result<Value, Error> {
        self.current()?;
        let all_reserved = self.identity.reserved.get().add(cost)?;
        let total = self.identity.committed.get().add(all_reserved)?;
        self.budget.check(total, depth)?;
        let own_reserved = self.reserved.add(cost)?;
        let id = self
            .identity
            .sequence
            .get()
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        self.staged.insert(id, Rc::new(Node { data, depth }));
        self.identity.sequence.set(id);
        self.identity.reserved.set(all_reserved);
        self.reserved = own_reserved;
        Ok(value(&self.identity, id))
    }
    fn scalar(&mut self, data: Data, bytes: usize) -> Result<Value, Error> {
        self.allocate(
            data,
            0,
            Usage {
                nodes: 1,
                edges: 0,
                bytes,
            },
        )
    }
    pub fn undefined(&mut self) -> Result<Value, Error> {
        self.scalar(Data::Undefined, 0)
    }
    pub fn null(&mut self) -> Result<Value, Error> {
        self.scalar(Data::Null, 0)
    }
    pub fn boolean(&mut self, boolean: bool) -> Result<Value, Error> {
        self.scalar(Data::Boolean(boolean), 0)
    }
    /// IEEE-754 Number, including NaN, infinity and signed zero. No JSON coercion.
    pub fn number(&mut self, number: f64) -> Result<Value, Error> {
        self.scalar(Data::Number(number), 0)
    }
    pub fn string(&mut self, text: String) -> Result<Value, Error> {
        let bytes = text.capacity();
        self.scalar(Data::String(text), bytes)
    }
    pub fn record(&mut self, fields: Vec<(String, Value)>) -> Result<Value, Error> {
        let outcome = self.record_inner(fields);
        self.finish(outcome)
    }
    fn record_inner(&mut self, fields: Vec<(String, Value)>) -> Result<Value, Error> {
        self.current()?;
        let bytes = fields.iter().try_fold(0usize, |total, (key, _)| {
            total.checked_add(key.capacity()).ok_or(Error::Overflow)
        })?;
        self.check_input_cost(Usage {
            nodes: 1,
            edges: fields.len(),
            bytes,
        })?;
        let mut nodes = BTreeMap::new();
        let mut depth = 0;
        let mut bytes = 0usize;
        for (key, child) in fields {
            depth = depth.max(self.node(&child)?.depth);
            bytes = bytes.checked_add(key.capacity()).ok_or(Error::Overflow)?;
            if nodes.insert(key, child.id).is_some() {
                return Err(Error::DuplicateField);
            }
        }
        let depth = depth.checked_add(1).ok_or(Error::Overflow)?;
        let cost = Usage {
            nodes: 1,
            edges: nodes.len(),
            bytes,
        };
        self.allocate(Data::Record(nodes), depth, cost)
    }
    pub fn array(&mut self, items: Vec<Value>) -> Result<Value, Error> {
        let outcome = self.array_inner(items);
        self.finish(outcome)
    }
    fn array_inner(&mut self, items: Vec<Value>) -> Result<Value, Error> {
        self.current()?;
        self.check_input_cost(Usage {
            nodes: 1,
            edges: items.len(),
            bytes: 0,
        })?;
        let mut depth = 0;
        let mut nodes = Vec::with_capacity(items.len());
        for child in items {
            depth = depth.max(self.node(&child)?.depth);
            nodes.push(child.id);
        }
        let depth = depth.checked_add(1).ok_or(Error::Overflow)?;
        let cost = Usage {
            nodes: 1,
            edges: nodes.len(),
            bytes: 0,
        };
        self.allocate(Data::Array(nodes), depth, cost)
    }
    pub fn same_value(&self, left: &Value, right: &Value) -> Result<bool, Error> {
        Ok(same_value(
            &self.node(left)?.data,
            &self.node(right)?.data,
            left.id == right.id,
        ))
    }
    fn check_input_cost(&self, cost: Usage) -> Result<(), Error> {
        self.budget.check(
            self.identity
                .committed
                .get()
                .add(self.identity.reserved.get())?
                .add(cost)?,
            0,
        )
    }
    fn release_reservation(&mut self) {
        self.identity
            .reserved
            .set(self.identity.reserved.get().subtract(self.reserved));
        self.reserved = Usage::default();
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        self.release_reservation();
        self.identity
            .candidates
            .set(self.identity.candidates.get() - 1);
    }
}

#[cfg(test)]
mod tests;

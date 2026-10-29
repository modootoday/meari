//! Issuance of root node leases is restricted to the already declared graph.
use super::{ComponentReference, SourceModuleGraph};
use crate::domain::{
    Value,
    node_slots::{NodeSlot, OwnedNodeSlot},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicU64, Ordering},
};

pub(super) fn identity() -> Result<u64, String> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        value.checked_add(1)
    })
    .map_err(|_| "node slot identity exhausted".into())
}
pub struct NodeSlotFactory<'a> {
    pub(super) graph: &'a SourceModuleGraph,
    pub(super) owner: u64,
    pub(super) active: &'a BTreeSet<u64>,
    pub(super) generation: u64,
}
impl NodeSlotFactory<'_> {
    /// Key identifies the component within one render placement; no source/HTML is evaluated.
    pub fn component(
        &self,
        key: &str,
        reference: ComponentReference,
        props: BTreeMap<String, Value>,
    ) -> Result<Value, String> {
        self.issue(key, reference, props, false)
    }

    /// Preserve identity when moving between slot positions under the same source parent.
    /// The key must be unique within that parent; the lease remains single-use per frame.
    pub fn movable_component(
        &self,
        key: &str,
        reference: ComponentReference,
        props: BTreeMap<String, Value>,
    ) -> Result<Value, String> {
        self.issue(key, reference, props, true)
    }

    fn issue(
        &self,
        key: &str,
        reference: ComponentReference,
        props: BTreeMap<String, Value>,
        movable: bool,
    ) -> Result<Value, String> {
        if !crate::domain::component_state::identifier(key) || key.len() > 256 {
            return Err("node slot requires a bounded identity key".into());
        }
        let definition = self.graph.definition(&reference)?;
        if props.len() != definition.component.props.len()
            || definition.component.props.iter().any(|prop| {
                !props
                    .get(&prop.name)
                    .is_some_and(|value| prop.kind.accepts(value))
            })
        {
            return Err("node slot requires exact declared component props".into());
        }
        validate_runtime(&props, self.owner, self.active, self.generation)?;
        Ok(Value::NodeSlot(NodeSlot::Owned(OwnedNodeSlot {
            owner: self.owner,
            nonce: identity()?,
            generation: self.generation,
            key: key.into(),
            movable,
            reference,
            props,
        })))
    }
}
pub(super) fn validate_runtime(
    props: &BTreeMap<String, Value>,
    owner: u64,
    active: &BTreeSet<u64>,
    generation: u64,
) -> Result<BTreeSet<u64>, String> {
    fn check(
        value: &Value,
        owner: u64,
        active: &BTreeSet<u64>,
        generation: u64,
        depth: usize,
        count: &mut usize,
        issued: &mut BTreeSet<u64>,
    ) -> Result<(), String> {
        *count += 1;
        if depth > 128 || *count > 100000 {
            return Err("runtime node slot budget exceeded".into());
        }
        match value{
            Value::NodeSlot(NodeSlot::Content(_) | NodeSlot::Captured(_))=>return Err("runtime node slots require an issued lease or explicit empty; raw authoring content is not authority".into()),
            Value::NodeSlot(NodeSlot::Owned(slot))=>{
                if slot.owner!=owner || (!active.contains(&slot.nonce) && slot.generation!=generation){return Err("foreign or retired node slot lease".into());}
                if !issued.insert(slot.nonce){return Err("owned node slot cannot be supplied more than once".into());}
                if issued.len()>16384{return Err("active node slot input budget exceeded".into());}
                for item in slot.props.values(){check(item,owner,active,generation,depth+1,count,issued)?;}
            }
            Value::Array(array)=>for item in &array.items{check(item,owner,active,generation,depth+1,count,issued)?;},
            Value::Record(record)=>for item in record.fields().values(){check(item,owner,active,generation,depth+1,count,issued)?;},
            _=>{}
        }
        Ok(())
    }
    let mut count = 0;
    let mut issued = BTreeSet::new();
    for item in props.values() {
        check(item, owner, active, generation, 0, &mut count, &mut issued)?;
    }
    Ok(issued)
}

//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use crate::domain::hook_value::{Data, same_value};

impl Transaction {
    pub(super) fn array_inner(&mut self, items: Vec<Value>) -> Result<Value, Error> {
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

    pub(super) fn check_input_cost(&self, cost: Usage) -> Result<(), Error> {
        self.budget.check(
            self.identity
                .committed
                .get()
                .add(self.identity.reserved.get())?
                .add(cost)?,
            0,
        )
    }

    pub(super) fn release_reservation(&mut self) {
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

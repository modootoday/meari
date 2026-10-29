use crate::domain::surface_slot::SurfaceError;
use std::{
    collections::BTreeMap,
    rc::{Rc, Weak},
};

/// Private issuer state: public metadata cannot mint or clone an active reservation.
#[derive(Default)]
pub(crate) struct SurfaceRegistry {
    next: u64,
    layout: u64,
    slots: BTreeMap<u64, (u64, Weak<()>)>,
}
pub(crate) struct Reservation {
    node: u64,
    generation: u64,
    owner: Rc<()>,
}
impl SurfaceRegistry {
    pub fn reserve(&mut self, node: u64) -> Result<Reservation, SurfaceError> {
        self.slots.retain(|_, (_, owner)| owner.strong_count() != 0);
        if self.slots.contains_key(&node) {
            return Err(SurfaceError::Duplicate);
        }
        if self.slots.len() >= 64 {
            return Err(SurfaceError::Capacity);
        }
        let generation = self
            .next
            .checked_add(1)
            .ok_or(SurfaceError::CounterExhausted)?;
        let owner = Rc::new(());
        self.slots.insert(node, (generation, Rc::downgrade(&owner)));
        self.next = generation;
        Ok(Reservation {
            node,
            generation,
            owner,
        })
    }
    pub fn validate(&self, reservation: &Reservation) -> Result<(), SurfaceError> {
        let Some((generation, owner)) = self.slots.get(&reservation.node) else {
            return Err(SurfaceError::Revoked);
        };
        if *generation != reservation.generation
            || !Weak::ptr_eq(owner, &Rc::downgrade(&reservation.owner))
        {
            return Err(SurfaceError::Stale);
        }
        Ok(())
    }
    pub fn next_layout(&mut self, reservation: &Reservation) -> Result<u64, SurfaceError> {
        self.validate(reservation)?;
        self.layout = self
            .layout
            .checked_add(1)
            .ok_or(SurfaceError::CounterExhausted)?;
        Ok(self.layout)
    }
}
impl Reservation {
    pub fn generation(&self) -> u64 {
        self.generation
    }
}
#[cfg(test)]
#[path = "surface_slot/tests.rs"]
mod tests;

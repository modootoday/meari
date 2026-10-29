//! Shared value-commit semantics. Original node scopes and values are not SDK authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeGesture {
    Pointer(u128),
    Key(String),
}
#[derive(Debug, PartialEq, Eq)]
pub struct RangeValueCommit<O> {
    pub owner: O,
    pub interaction: u64,
    pub value: String,
}
struct Active<O> {
    owner: O,
    interaction: u64,
    gesture: RangeGesture,
    initial: String,
}
pub struct RangeInteraction<O> {
    next: u64,
    active: Option<Active<O>>,
}
impl<O> Default for RangeInteraction<O> {
    fn default() -> Self {
        Self {
            next: 0,
            active: None,
        }
    }
}
impl<O: PartialEq> RangeInteraction<O> {
    pub fn owner(&self) -> Option<&O> {
        self.active.as_ref().map(|active| &active.owner)
    }
    pub fn gesture(&self) -> Option<&RangeGesture> {
        self.active.as_ref().map(|active| &active.gesture)
    }
    pub fn changed_from_initial(&self, actual: &str) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| active.initial != actual)
    }
    pub fn begin(&mut self, owner: O, gesture: RangeGesture, actual: &str) -> Result<(), String> {
        if actual.len() > 1024 * 1024 {
            return Err("range value exceeds byte budget".into());
        }
        if let RangeGesture::Key(key) = &gesture
            && ![
                "ArrowLeft",
                "ArrowRight",
                "ArrowUp",
                "ArrowDown",
                "Home",
                "End",
                "PageUp",
                "PageDown",
            ]
            .contains(&key.as_str())
        {
            return Err("key is not a range adjustment gesture".into());
        }
        if matches!(&gesture, RangeGesture::Key(_))
            && self
                .active
                .as_ref()
                .is_some_and(|active| active.owner == owner && active.gesture == gesture)
        {
            return Ok(());
        }
        self.cancel();
        self.next = self
            .next
            .checked_add(1)
            .ok_or("range interaction sequence exhausted")?;
        self.active = Some(Active {
            owner,
            interaction: self.next,
            gesture,
            initial: actual.into(),
        });
        Ok(())
    }
    pub fn cancel(&mut self) {
        self.active = None;
    }
    pub fn retain_owner(&mut self, owner: &O) -> bool {
        if self
            .active
            .as_ref()
            .is_some_and(|active| &active.owner != owner)
        {
            self.cancel();
        }
        self.active.is_some()
    }
    /// Adapter must read the actual sanitized owned value, not an earlier captured draft.
    /// Downstream failure must retain this original returned value, not reconstruct a new intent.
    pub fn finish(
        &mut self,
        owner: &O,
        gesture: &RangeGesture,
        actual: &str,
    ) -> Result<Option<RangeValueCommit<O>>, String> {
        if actual.len() > 1024 * 1024 {
            return Err("range value exceeds byte budget".into());
        }
        if !self.retain_owner(owner) {
            return Ok(None);
        }
        if &self.active.as_ref().expect("retained interaction").gesture != gesture {
            return Ok(None);
        }
        let active = self.active.take().expect("matching interaction");
        if active.initial == actual {
            return Ok(None);
        }
        Ok(Some(RangeValueCommit {
            owner: active.owner,
            interaction: active.interaction,
            value: actual.into(),
        }))
    }
}
#[cfg(test)]
mod tests;

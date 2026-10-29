use crate::domain::window_lifecycle::{LifecycleError, WindowStamp};
use std::{
    fmt,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

pub(super) struct Authority {
    pub id: u64,
}
impl Authority {
    pub fn new() -> Result<Rc<Self>, LifecycleError> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| LifecycleError::CounterExhausted)?;
        Ok(Rc::new(Self { id }))
    }
}

/// Non-cloneable, non-serializable UI-thread authority. Its metadata is not authority.
pub struct ChildLease {
    pub(super) authority: Rc<Authority>,
    pub(super) window: WindowStamp,
    pub(super) child: u64,
}
impl ChildLease {
    /// Internal derived authority for a parent-access adapter, never a public Clone operation.
    #[cfg(any(feature = "window", test))]
    pub(crate) fn for_parent(&self) -> Self {
        Self {
            authority: self.authority.clone(),
            window: self.window,
            child: self.child,
        }
    }
    pub fn window(&self) -> WindowStamp {
        self.window
    }
    pub fn id(&self) -> u64 {
        self.child
    }
}
impl fmt::Debug for ChildLease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChildLease")
            .field("window", &self.window)
            .field("child", &self.child)
            .finish()
    }
}

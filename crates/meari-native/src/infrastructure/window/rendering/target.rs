//! Recheck physical geometry immediately before presentation, after raster/encode.
use std::sync::Arc;

#[derive(Default)]
pub(super) struct CurrentTarget(Option<Arc<dyn Fn() -> (u32, u32) + Send + Sync>>);
impl CurrentTarget {
    pub fn bind(&mut self, window: Arc<dyn winit::window::Window>) {
        self.0 = Some(Arc::new(move || {
            let size = window.surface_size();
            (size.width, size.height)
        }));
    }
    pub fn accepts(&self, size: (u32, u32)) -> bool {
        size.0 != 0 && size.1 != 0 && self.0.as_ref().is_none_or(|current| current() == size)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    #[test]
    fn size_change_during_raster_rejects_old_frame_before_present() {
        let clock = Arc::new(AtomicU64::new((800 << 32) | 600));
        let observed = clock.clone();
        let target = CurrentTarget(Some(Arc::new(move || {
            let packed = observed.load(Ordering::Acquire);
            ((packed >> 32) as u32, packed as u32)
        })));
        assert!(target.accepts((800, 600)));
        clock.store((900 << 32) | 700, Ordering::Release);
        assert!(!target.accepts((800, 600)));
        assert!(target.accepts((900, 700)));
        clock.store(0, Ordering::Release);
        assert!(!target.accepts((900, 700)));
        assert!(!target.accepts((0, 0)));
    }
}

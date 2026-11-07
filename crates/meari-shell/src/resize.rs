//! One native pending size, never an unbounded queue of obsolete layouts.
use winit::dpi::PhysicalSize;

#[derive(Default)]
pub(crate) struct LatestResize(Option<PhysicalSize<u32>>);
impl LatestResize {
    pub fn replace(&mut self, size: PhysicalSize<u32>) {
        self.0 = Some(size);
    }
    pub fn take(&mut self) -> Option<PhysicalSize<u32>> {
        self.0.take()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resize_burst_keeps_only_the_latest_including_minimize() {
        let mut pending = LatestResize::default();
        for width in 1..10_000 {
            pending.replace(PhysicalSize::new(width, 480));
        }
        assert_eq!(pending.take(), Some(PhysicalSize::new(9_999, 480)));
        assert_eq!(pending.take(), None);
        pending.replace(PhysicalSize::new(100, 100));
        pending.replace(PhysicalSize::new(0, 0));
        assert_eq!(pending.take(), Some(PhysicalSize::new(0, 0)));
    }
}

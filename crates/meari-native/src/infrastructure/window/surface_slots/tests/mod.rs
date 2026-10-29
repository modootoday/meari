use super::*;
#[derive(Clone, Copy)]
struct Clock;
struct Child;
struct RejectAccess(Rc<RefCell<Option<CleanupRequest>>>);
struct PaintContext {
    window: WindowStamp,
    revision: Rc<Cell<u64>>,
}

mod renderer_fakes;
use self::renderer_fakes::*;
#[cfg(test)]
mod safe_area;
#[cfg(test)]
mod scene_pixels;

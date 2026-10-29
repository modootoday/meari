use crate::application::window_lifecycle::MonotonicClock;
use std::time::{Duration, Instant};
#[derive(Clone, Copy)]
pub(crate) struct NativeClock {
    pub origin: Instant,
}
impl NativeClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}
impl MonotonicClock for NativeClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}
pub(crate) fn apply_deadline(
    event_loop: &dyn winit::event_loop::ActiveEventLoop,
    next: Option<Instant>,
    owned: &mut Option<(Instant, winit::event_loop::ControlFlow)>,
) {
    // The pinned Win32 backend waits once after AboutToWait even if that callback
    // requested exit. A zero wait lets it observe exit without another OS input.
    if event_loop.exiting() {
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
        *owned = None;
        return;
    }
    let current = event_loop.control_flow();
    let (desired, next_owned) = select_deadline(current, next, *owned);
    if current != desired {
        event_loop.set_control_flow(desired);
    }
    *owned = next_owned;
}
fn select_deadline(
    current: winit::event_loop::ControlFlow,
    next: Option<Instant>,
    owned: Option<(Instant, winit::event_loop::ControlFlow)>,
) -> (
    winit::event_loop::ControlFlow,
    Option<(Instant, winit::event_loop::ControlFlow)>,
) {
    use winit::event_loop::ControlFlow;
    let base = owned
        .filter(|(last, _)| current == ControlFlow::WaitUntil(*last))
        .map_or(current, |(_, previous)| previous);
    let selected = next.filter(|deadline| match base {
        ControlFlow::Wait => true,
        ControlFlow::WaitUntil(other) => *deadline < other,
        ControlFlow::Poll => false,
    });
    let desired = selected.map_or(base, ControlFlow::WaitUntil);
    (desired, selected.map(|deadline| (deadline, base)))
}
#[cfg(test)]
mod tests {
    use super::*;
    use winit::event_loop::ControlFlow;
    #[test]
    fn external_poll_and_earlier_deadline_are_preserved_and_owned_timer_is_restored() {
        let now = Instant::now();
        let early = now + Duration::from_millis(5);
        let late = now + Duration::from_millis(10);
        assert_eq!(
            select_deadline(ControlFlow::Poll, Some(early), None),
            (ControlFlow::Poll, None)
        );
        assert_eq!(
            select_deadline(ControlFlow::WaitUntil(early), Some(late), None),
            (ControlFlow::WaitUntil(early), None)
        );
        let (flow, owned) = select_deadline(ControlFlow::WaitUntil(late), Some(early), None);
        assert_eq!(flow, ControlFlow::WaitUntil(early));
        assert_eq!(
            select_deadline(flow, None, owned),
            (ControlFlow::WaitUntil(late), None)
        );
    }
    #[test]
    fn host_and_child_absolute_deadlines_share_min_without_origin_confusion() {
        let origin = Instant::now();
        let child_origin = origin + Duration::from_millis(20);
        let host = origin.checked_add(Duration::from_millis(50)).unwrap();
        let child = child_origin.checked_add(Duration::from_millis(5)).unwrap();
        let earliest = [host, child].into_iter().min();
        assert_eq!(earliest, Some(child));
        assert_eq!(
            select_deadline(ControlFlow::Wait, earliest, None).0,
            ControlFlow::WaitUntil(child)
        );
    }
}

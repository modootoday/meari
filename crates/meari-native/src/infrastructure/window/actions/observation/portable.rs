//! Platforms without a native window-state probe report unknown capabilities.
use crate::application::window_actions::{ActionCapabilities, WindowObservation};
use winit::window::Window;

pub(crate) fn observation(_window: &dyn Window) -> (ActionCapabilities, WindowObservation) {
    (ActionCapabilities::default(), WindowObservation::default())
}

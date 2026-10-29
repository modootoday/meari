//! Window-state observation through the platform port; OS adapters live in `meari-platform`.
use crate::application::window_actions::{ActionCapabilities, WindowObservation};
use meari_platform::OsPolicy;
use winit::{raw_window_handle::HasWindowHandle, window::Window};

pub(crate) fn observation(window: &dyn Window) -> (ActionCapabilities, WindowObservation) {
    let state = window
        .window_handle()
        .ok()
        .and_then(|handle| meari_platform::current().window_state(handle.as_raw()));
    match state {
        Some(state) => (
            ActionCapabilities {
                minimize: true,
                maximize: true,
                borderless_fullscreen: state.borderless_monitor_coverage.is_some(),
            },
            WindowObservation {
                minimized: state.minimized,
                maximized: state.maximized,
                borderless_monitor_coverage: state.borderless_monitor_coverage,
            },
        ),
        None => (ActionCapabilities::default(), WindowObservation::default()),
    }
}

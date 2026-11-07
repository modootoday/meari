//! Content-free timings. No text, input, AX nodes, or scene commands are retained here.
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default)]
pub struct WindowFrameTimings {
    /// Receipt of the latest resize event to the start of its layout work.
    /// The OS's event enqueue timestamp is not available through winit.
    pub event_wait: Option<Duration>,
    /// Responsive source projection, excluding source parsing by the host.
    pub source_update: Duration,
    pub layout: Duration,
    pub scene_build: Duration,
    /// CPU raster, or CPU time encoding/submitting GPU raster (not GPU execution).
    pub raster: Duration,
    pub pixel_copy: Duration,
    /// Surface acquisition/configuration; may include platform throttling.
    pub surface_acquire: Duration,
    pub present_call: Duration,
    pub resize_to_present: Option<Duration>,
    /// Monotonic present-return timestamp for correlation with host resize requests.
    pub present_returned_at: Option<Instant>,
    /// None: neither softbuffer nor portable wgpu exposes compositor display ACK.
    /// Queue completion MUST NOT be substituted for actual display.
    pub resize_to_displayed: Option<Duration>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RenderTimings {
    pub raster: Duration,
    pub pixel_copy: Duration,
    pub surface_acquire: Duration,
    pub present_call: Duration,
    pub present_returned_at: Option<Instant>,
}

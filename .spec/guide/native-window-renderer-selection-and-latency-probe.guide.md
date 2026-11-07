---
id: guide:meari/native-window-renderer-selection-and-latency-probe
title: Native window renderer selection and latency probe
status: active
created: 2026-11-07T17:44:55Z
updated: 2026-11-07T18:23:25Z
rel:
  governs: [crates/meari-native]
---

# Native window renderer selection and latency probe

## Selection


`meari-native` keeps CPU rendering with `features = ["window"]`. Add
`features = ["window-gpu"]` to opt into the GPU implementation (no `gl` needed).

```rust,ignore
use meari_native::infrastructure::window::WindowRendererChoice;
let prepared = NativeWindowPreparation::new(event_loop, document, limits, options, observer)?
    .with_renderer(WindowRendererChoice::Gpu)?;
let driver = prepared.attach(runtime)?;
```

Plain runtimes can call `NativeWindowDriver::attach_with_renderer(...)`. Prepared
startup attachments can select with the same `with_renderer` before attachment.
`NativeWindowRenderer` is also exported for direct shell embedders.

Read `driver.renderer_status()` after surface creation: `selected` is `None` before
successful initialization; `error` is explicit on initialization failure, device
loss or fatal rendering failure. `Gpu` rejects a CPU/software GPU adapter.
`GpuAllowSoftware` permits software adapters for diagnostics; those adapters report
`software_fallback = true` and must not be described as hardware GPU success. No automatic CPU fallback is performed.
Neither GPU choice is accepted without the feature. Selection cannot change a live
surface; suspend/create a new attachment for backend changes.


Direct shell users should bind the window with `renderer.observe_window(window.clone())`
before resume; the native driver does this automatically. This adds a final physical-size
check after raster/encode, immediately before presenting. If the size changed during the
frame, the frame is discarded (GPU resources still retire through the completion callback).

## Work and resource ownership

Native resize events replace one pending size. The latest size is applied before
layout/checked scene construction; input flushes pending geometry before hit testing.
Safe-area arithmetic saturates. A zero-size/minimized surface does not submit a
frame. Layout and checked submission hold exclusive UI-thread ownership, so an old
size cannot be submitted after a newer-size frame. No asynchronous queue of layouts
or recorded scenes is created.

GPU raster stays on the device: Vello renders an RGBA storage texture and a GPU blit
copies it into the window swapchain. There is no pixel readback. The optional Skia
`gl` host is unchanged and is not this window path.

One GPU frame may be in flight. Nonblocking `Device::poll(PollType::Poll)` observes
a queue-completion callback; while busy, redraw intent and latest geometry are
retained rather than building another frame. Completion polling uses a 2ms event-loop
deadline, not a UI-thread GPU wait. GPU raster scene and textures remain captured
until completion, while Vello's caches remain in the active owner. Resize configure
and target replacement occur only after completion. A 2s completion timeout stops
the renderer and reports an error, never grants permission to reuse the slot.

During suspension/destruction the native surface is released on the UI thread;
in-flight device caches are retired on a bounded off-thread drain. On drain failure
the device is cancelled before dropping those caches; wgpu owns references to every
submitted GPU allocation. No GPU wait was simply removed without lifetime ownership.
Surface acquire/configure/present are still platform calls and may throttle; their
time is reported separately. This is not a guarantee of a stall-free graphics driver.

## Size-only API and integration

`InteractiveDocument::refresh_window_layout(HostEnvironment)` accepts an observed
host geometry without parsing/recompiling source or replacing the persistent DOM.
It keeps existing input/focus state and DOM identity on size-only updates. Unchanged
media queries do not trigger a source projection; responsive media-bound state may
still project an actual change using the existing retained-source contract. Click metadata and AccessKit use this same document.
It is not permission to acknowledge an unobserved OS resize; requested resizes still
use the existing ScreenRequest acknowledgement contract.

The renderer does not replace window creation, parent/child handle attachment,
AccessKit ownership, IME/focus handling, DPI/zoom, window actions or shutdown
coordination. Native Windows child-window and Wayland integration still require
platform verification; do not infer them from Linux compilation. No sandbox flags
or privileges are changed. Custom renderer resources are forwarded on the CPU path;
the new GPU adapter rejects unsupported custom-resource registration explicitly.
The published GPU painter omits filter/backdrop-filter and custom/resource brushes;
this adapter explicitly rejects those scenes before submission instead of silently
changing their appearance. Keep such documents on CPU until GPU support is added.

## Timings and reproducible probe

`driver.last_frame_timings()` returns content-free `WindowFrameTimings`. Event wait
starts at the latest winit resize-event receipt, **not OS event enqueue**. Source
update measures responsive source projection (not host-side source compilation).
Layout includes responsive environment application and final asset/layout resolve;
scene build, raster, pixel-copy, surface acquire/configure and present-call have
separate clocks. GPU raster is CPU encoding/submission time, not GPU execution.
CPU pixel-copy measures RGBA → softbuffer conversion; GPU pixel-copy is zero.
Stage durations describe a successful attempt, not all discarded attempts; end-to-end
latency includes any waits/retries. Quantiles from separate stages must not be summed.

`resize_to_present` ends after present-call return. `resize_to_displayed` is `None`:
portable wgpu/softbuffer provide no compositor display acknowledgement here. Queue
completion is resource-lifetime evidence, **not actual display**. No input contents,
page text, raw AX text or scene commands are collected by these timings.

```sh
cargo run -j 1 --release -p meari-native-tools --features window --bin meari-resize-probe -- cpu
cargo run -j 1 --release -p meari-native-tools --features window-gpu --bin meari-resize-probe -- gpu
# Optional diagnostic, never hardware evidence:
cargo run -j 1 --release -p meari-native-tools --features window-gpu --bin meari-resize-probe -- gpu-software
```

Use the same optimized binary feature set for both CPU and GPU when comparing.
The additional `resize_request_to_present_return` row correlates producer request
clocks with the presented size and includes dispatch backlog. Neither row is a
compositor-visible latency measurement.

The probe creates a synthetic 64-control window, warms up for 250ms, then an
independent producer requests 240 sizes at 8ms intervals: width
`640 + (step*37)%400`, height `400 + (step*23)%240`. It reports p50/p95/p99/max
(nearest rank), backend/fallback status, and presented resize sample counts.
Coalesced events intentionally have no presented sample. Run each configuration at
least three times under the same compositor/workload and report run spread; do not
interpret these synthetic present-return numbers as user application visible latency.

Validation observations: [evidence](../evidence/20261107174455-native-window-rendering-validation/index.evidence.md).


The synthetic fixture explicitly selects the Light theme through window attributes;
X11 does not report an OS color scheme. Host snapshots honor an explicit application
theme but still reject an unavailable system theme when no override was selected.
Only presentations matching a generated request enter the sample set.
`resize_burst_inter_present_gap` includes the interval from the first resize request
to the first successful resize presentation, then gaps between resize presentations.
This supplementary starvation metric prevents coalesced samples from hiding a long
period with no presentation. It is not resize-to-display latency. Counts are mandatory:
small samples cannot establish reliable tail percentiles.

Warmup starts only after an initial window presentation and GPU completion (when relevant), so cold shader initialization is outside the resize burst. The probe fails if it cannot finish all 240 requests and present the final size, or observes older request sizes after newer presented frames.

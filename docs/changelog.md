---
title: Changelog
description: "What changed in each Meari release, and what to do when you upgrade."
---

# Changelog

All Meari crates are released together under one version. This page explains what each release means for your code; the full list is in [CHANGELOG.md](https://github.com/modootoday/meari/blob/main/CHANGELOG.md).

## 0.3.0 (2026-11-08)

This release adds an experimental GPU path for native windows and makes resizing follow the pointer more closely.

- **New**: with the `window-gpu` feature, `meari-native` can render a window on the GPU. Pick it with `NativeWindowPreparation::with_renderer(WindowRendererChoice::Gpu)` or `NativeWindowDriver::attach_with_renderer(...)`. The default `window` path is still the CPU renderer, and there is no automatic CPU fallback.
- **New**: `NativeWindowDriver::renderer_status()` reports the selected backend, adapter and errors, and `last_frame_timings()` reports the stages of the last frame. A returned present call is not proof that the frame reached the display.
- **New**: `meari-resize-probe` in `meari-native-tools` measures synthetic resize latency so you can compare the CPU and GPU paths.
- **Changed**: resize requests now coalesce to the latest size, and frames drawn for an older size are rejected. The GPU path keeps at most one frame in flight.
- **Limitation**: the GPU path reports an error for `filter`, `backdrop-filter` and custom brushes instead of silently dropping them. Keep such documents on the CPU renderer.
- **Upgrade note**: `meari-shell::View` now has private resize state on native targets, so you can no longer build it with a struct literal. Use the shell's window creation APIs.

## 0.2.0 (2026-10-31)

This release lets apps that draw their own title bar resize their window like any other.

- **New**: `WindowAction::BeginResize(ResizeDirection)` in `meari-native` asks the operating system to start an interactive resize from an edge or corner. It completes as applied when the OS accepts and fails with `ActionError::Unsupported` when it does not, as on macOS.
- **Upgrade note**: `WindowAction` gained a variant. If you `match` on it exhaustively, add a `BeginResize` arm. That is why this is 0.2 rather than 0.1.1.
- **Upgrade note**: `meari-shell` no longer enables `accessibility`, `clipboard` and `file-dialog` by default. Turn on the ones you use. With all of them on, a Meari window got two accessibility adapters and crashed on Windows.

## 0.1.0 (2026-10-30)

The first release on crates.io.

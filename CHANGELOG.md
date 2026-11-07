# Changelog

All crates in this workspace share one version (`[workspace.package] version`).

## [0.3.0] - 2026-11-08

### Added
- `meari-native`: experimental opt-in GPU window rendering with `window-gpu` and
  `NativeWindowPreparation::with_renderer(WindowRendererChoice::Gpu)` or
  `NativeWindowDriver::attach_with_renderer(...)`. The default `window` path remains
  CPU (`VelloCpu`). Hardware-only GPU selection rejects software adapters; diagnostic
  software selection is explicit, with no automatic CPU fallback.
- `NativeWindowDriver::renderer_status()` reports the selected backend, software fallback,
  adapter and errors; `last_frame_timings()` reports individual frame stages.
  Present-call return and GPU queue completion are not actual-display acknowledgements.
- `InteractiveDocument::refresh_window_layout` refreshes persistent-DOM layout without
  source recompilation. Unchanged media queries no longer reproject source.
- `meari-native-tools`: `meari-resize-probe` measures synthetic resize request-to-present
  return latency, coalesced sample counts and presentation gaps for CPU/GPU comparison.
- Public documentation site with getting-started, app, verification and contributing guides,
  a crates layout page, Markdown twins and LLM indexes, plus a product-token icon and favicons.

### Changed
- Native resize requests coalesce to the latest size; stale-size frames are rejected.
  The GPU path keeps at most one frame in flight and retains resources until completion,
  rendering to the window surface without CPU pixel readback. No sandbox privileges change.
- Distribution documentation consistently describes cargo/crates.io; removed npm references
  (Meari was never published through npm).

### Changed (breaking)
- `meari-shell::View` now has private resize state on native targets. Downstream native
  struct-literal construction is no longer possible; use the shell's window creation APIs.

### Limitations
- GPU rendering explicitly errors on unsupported `filter`/`backdrop-filter` and
  custom/resource brushes rather than silently omitting them. Keep such documents on CPU.

## [0.2.0] - 2026-10-31

### Added
- `meari-native`: `WindowAction::BeginResize(ResizeDirection)` starts an OS-driven
  interactive resize from an edge or corner (winit `drag_resize_window`), for apps that draw
  their own title bar and borders. Accepted without window-state observation; it completes as
  `Applied` when the OS accepts the request and fails with `ActionError::Unsupported` when it
  does not (e.g. macOS). See `adr:meari/20261031054818`.

### Changed (breaking)
- `WindowAction` gained a variant and is not `#[non_exhaustive]`: an exhaustive `match` on it
  outside this crate must handle `BeginResize`. Hence 0.1 → 0.2.
- `meari-shell` has no default features (upstream defaulted to `accessibility`, `clipboard`,
  `file-dialog`); enable what you use. With the workspace default on, `meari-native` windows
  got a second AccessKit adapter and Windows panicked. See `adr:meari/20261031070804`.

## [0.1.0] - 2026-10-30

First crates.io release.

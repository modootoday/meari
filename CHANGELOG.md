# Changelog

All crates in this workspace share one version (`[workspace.package] version`).

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

### Removed
- npm distribution (never published); meari ships through cargo/crates.io only
  (`adr:meari/20261031023000`).

## [0.1.0] - 2026-10-30

First crates.io release.

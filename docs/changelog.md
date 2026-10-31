---
title: Changelog
description: "What changed in each Meari release, and what to do when you upgrade."
---

# Changelog

All Meari crates are released together under one version. This page explains what each release means for your code; the full list is in [CHANGELOG.md](https://github.com/modootoday/meari/blob/main/CHANGELOG.md).

## 0.2.0 (2026-10-31)

This release lets apps that draw their own title bar resize their window like any other.

- **New**: `WindowAction::BeginResize(ResizeDirection)` in `meari-native` asks the operating system to start an interactive resize from an edge or corner. It completes as applied when the OS accepts and fails with `ActionError::Unsupported` when it does not, as on macOS.
- **Upgrade note**: `WindowAction` gained a variant. If you `match` on it exhaustively, add a `BeginResize` arm. That is why this is 0.2 rather than 0.1.1.
- **Upgrade note**: `meari-shell` no longer enables `accessibility`, `clipboard` and `file-dialog` by default. Turn on the ones you use. With all of them on, a Meari window got two accessibility adapters and crashed on Windows.

## 0.1.0 (2026-10-30)

The first release on crates.io.

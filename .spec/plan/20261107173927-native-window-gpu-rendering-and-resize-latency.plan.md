---
id: plan:meari/20261107173927
title: Native window GPU rendering and resize latency
status: active
created: 2026-11-07T17:39:27Z
updated: 2026-11-07T18:06:59Z
rel:
  governs: [crates/meari-native/src/infrastructure/window]
---

# Native window GPU rendering and resize latency

## Goal

Opt-in surface GPU rendering, latest-size layout, bounded GPU work and content-free latency evidence. Keep the CPU default and existing window lifecycle.

## Steps

1. Verify upstream traits and GPU synchronization; introduce an observable adapter.
2. Coalesce native viewport events before layout and checked scene submission.
3. Instrument stages and add a repeatable optimized resize probe.
4. Run correctness gates and compare repeated release runs; document unsupported display acknowledgement and platform coverage.

## Remaining validation

Hardware GPU performance and device-loss injection, actual compositor display acknowledgement, Windows CEF child HWND/focus/IME/DPI/maximize/restore/minimize/shutdown integration, and Wayland remain platform follow-ups. A software GPU diagnostic is never hardware performance evidence. The reported application leak has not been reproduced or diagnosed by this resize probe.

## Record

Baseline is commit 1482b74. Reported 150–200ms is user perception, not measured evidence. The CPU adapter renders through RGBA and softbuffer. anyrender_vello 0.14 uses anyrender 0.13 (incompatible); 0.15 uses anyrender 0.14 and wgpu 30. Its stock window renderer waits indefinitely after every frame, so reuse its published scene painter, not that window loop. No bottleneck percentage or speedup has been established.

A focused regression reproduced unnecessary responsive-source reprojection for a
size-only update with unchanged media queries: the source-patch path was visited
and failed on a deliberately removed projected target. The empty-change fast path
now skips that projection; a revision-gap test checks later media changes still apply.
Validation and measurement results belong in
[evidence](../evidence/20261107174455-native-window-rendering-validation/index.evidence.md).

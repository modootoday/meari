---
id: change:meari/20261107183145
title: Native window renderer and latency instrumentation
status: active
created: 2026-11-07T18:31:45Z
updated: 2026-11-07T18:31:52Z
rel:
  depends_on: [sot:meari/fonts]
---

# Native window renderer and latency instrumentation

Adds experimental opt-in GPU surface rendering, explicit backend/fallback/error status, latest-size resize work, persistent-DOM layout refresh, and content-free staged timings/probe. CPU remains the default. No font catalog, delivery, licensing, or font data policy changes are made; the font source-of-truth governs the broad native crate paths touched by this renderer change.

See the [API and lifetime guide](../guide/native-window-renderer-selection-and-latency-probe.guide.md) and [validation evidence](../evidence/20261107174455-native-window-rendering-validation/index.evidence.md) for exact APIs, repeated measurements, sparse-sample limitations and platform follow-ups. Hardware GPU performance and actual-display latency are not established on this Linux box. No version bump, tag, release or publication is included.

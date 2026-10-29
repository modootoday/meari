# Meari paint backend correction

This is a source-pinned fork of the published Rust `blitz-paint` 0.3.0-beta.4
package, from DioxusLabs/blitz revision
`92b11f1bd8ab7975411328b0406a95a09445b4e7`, directory `packages/blitz-paint`.
The original package identity and MIT OR Apache-2.0 licenses are retained.
See [upstream](https://github.com/DioxusLabs/blitz/tree/92b11f1bd8ab7975411328b0406a95a09445b4e7/packages/blitz-paint).

Meari changes SVG background/mask image layers to repeat vector tiles using
the same origin, clipping, positioning and per-axis repetition rules as other
CSS image layers. The upstream path painted one SVG tile regardless of repeat.
`paint_scene_checked` reports the bounded vector tile budget instead of
publishing an incomplete frame as successful. The offscreen CPU and GL paint
adapters use this checked entry point. `prepare_scene_checked` records a validated
vector scene and reports unsupported custom/resource paint instead of silently
recording transparency. Native first-frame and redraw adapters now call the
checked prepare/replay path; this new window integration is awaiting execution
verification and no GUI acceptance is claimed. Scoped resource recording remains
additional work.
Generated Native applications use the same path dependency and registry patch.

The fork is a renderer implementation dependency, not a replacement UI SDK.
It does not claim complete CSS, font, accessibility or browser parity.

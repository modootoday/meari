# Meari DOM layout correction

This is a source-pinned fork of the published Rust `blitz-dom` 0.3.0-beta.4
package, from DioxusLabs/blitz revision
`92b11f1bd8ab7975411328b0406a95a09445b4e7`, directory `packages/blitz-dom`.
The original package identity and MIT OR Apache-2.0 licenses are retained.
See [upstream](https://github.com/DioxusLabs/blitz/tree/92b11f1bd8ab7975411328b0406a95a09445b4e7/packages/blitz-dom).

Meari changes (decision record: `adr:meari/20261030035135`):

- **Inline width rounding.** Upstream rounds the inline content width and the
  measured inline size up to a whole pixel (`.ceil()`). Chrome rounds up to a
  1/64px `LayoutUnit` (`LayoutUnit::FromFloatCeil`). Both points in
  `src/layout/inline/` now use `layout_unit_ceil`, marked with `Meari:`
  comments, so right-aligned text keeps Chrome's position.
- **Platform queries.** OS `cfg` checks (macOS action modifier, macOS
  Backspace handling, `-apple-system`/`BlinkMacSystemFont` aliases) ask
  `meari-platform` for the current OS family instead (`src/util.rs`). iOS is
  approximated by the macOS answer.
- **Source layout only.** To meet Meari's lint-structure rules, files over 600
  lines were split item by item into `x/mod.rs` plus child modules, a few long
  bodies were moved into helper functions verbatim, and inline test modules
  were moved to `tests/` files. Top-level modules are stored in the grouping
  folders `styling/`, `content/` and `tree_access/` and loaded with `#[path]`.
  Module paths, public API and behaviour are unchanged.

The fork is a layout implementation dependency, not a replacement UI SDK.
It does not claim complete CSS or browser parity.

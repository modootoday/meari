# Meari

> **Write once, echo everywhere.**

Meari (메아리, "echo"; Multi-platform Experience Across Renderers Interface) is a Rust UI framework and WASM-based Native+Web builder under development. Meari shares typed UI contracts across source conversion, native rendering and browser adapters. CLI tools exercise those same APIs.

[![crates.io](https://img.shields.io/crates/v/meari.svg)](https://crates.io/crates/meari) [![docs.rs](https://docs.rs/meari/badge.svg)](https://docs.rs/meari) · [Documentation](https://modootoday.github.io/meari/)

## Install

Meari ships through Cargo and crates.io only. All crates share one version (0.2.0, see [CHANGELOG](CHANGELOG.md)).

```text
cargo install meari-cli          # the unified `meari` command: convert, native, build, fonts
meari fonts fetch                # one explicit download of the pinned Noto fonts
```

Library users add `meari` (and `meari-native` for native rendering) to `Cargo.toml`. On Linux, building the native crates needs the fontconfig and FreeType development packages and `pkg-config` (Debian/Ubuntu: `libfontconfig1-dev libfreetype-dev pkg-config`).

## Packages

| Package | Responsibility |
| --- | --- |
| `meari` | Portable domain/application, bounded Rust/TSX conversion, shared media/HMR contracts, font manifest, optional `wasm` adapter |
| `meari-platform` | OS port; every OS-specific `cfg` lives here, selected once through `CurrentOs` |
| `meari-native` | Native CSS/layout/paint, widget interactions, window actions, accessibility and OS adapters |
| `meari-native-tools` | Executable hosts `meari-native` and `meari-kitchen-sink` (showcase/CLI and batch verifier) |
| `meari-builder` | Host-side project loading, registered compiler adapters, immutable app bundles and build/serve CLI |
| `meari-cli` | The unified `meari` command line |
| `meari-dom`, `meari-paint` | Source-pinned forks of `blitz-dom` / `blitz-paint` 0.3.0-beta.4 with Meari layout and paint corrections |
| `meari-html`, `meari-shell` | Vendored copies of `blitz-html` / `blitz-shell` 0.3.0-beta.4, republished so they build against the forks without `[patch]` |

The packages live under `crates/` in one Cargo workspace (unpublished `meari-visual`, `meari-demo` and `xtask` are workspace-only). The default member is `meari`; native dependencies are opt-in. Web adapters currently use `meari`'s `wasm` feature. A separate `meari-web` package is reserved for a future independent Web runtime, not an empty placeholder.

```text
cargo test -j1
cargo clippy -j1 --workspace --all-targets -- -D warnings
cargo test -j1 -p meari-native -- --test-threads=1
cargo check -p meari --target wasm32-unknown-unknown --features wasm
cargo run -p meari-cli -- --help
cargo run -p meari-native-tools --bin meari-native -- --help
cargo run -p meari-builder -- --help
cargo xtask visual-test --gate strict
```

## Current support

The core exposes `parse`, `emit`, `convert`, `prepare_native` and injectable application ports. Generated Rust imports `meari::runtime`. Typed string/bool props, bounded `useId`, source mappings and diagnostics are supported. General React hooks/effects/events/control flow and arbitrary Rust/React equivalence remain incomplete.

Tailwind CSS and shadcn/ui are the default design profile. Native adapters consume compiled CSS and source-owned widget contracts. Direct custom CSS, CSS-first Tailwind configuration, semantic SVG Icon tokens, persistent input/focus and selected composite interactions are implemented. `WindowAction::BeginResize(ResizeDirection)` (0.2.0) starts an OS-driven edge/corner resize for apps that draw their own title bar; it reports `ActionError::Unsupported` where the OS refuses (e.g. macOS). Complete CSS/provider parity and full per-OS native acceptance remain unfinished.

The `wasm` feature exposes conversion APIs, browser media-query evaluation and the Rust `WebDocument` adapter for persistent DOM. A Web host supplies a container, component, typed props, CSS and a unique document scope/epoch; the adapter owns its listeners and mount lifetime. Complete Web component recipes and the Native+Web builder remain unfinished. Shared dual-target HMR coordination and retained source commit/rollback adapters exist; browser editor transport and coordinated commits of both running hosts remain unfinished. See [architecture and support boundaries](.spec/sot/architecture-and-support-boundaries.sot.md) and [runtime lifecycle](.spec/sot/runtime-lifecycle-and-verification-boundaries.sot.md).

## Native verification

Primitive source `useState` declarations, scalar setter expressions and interleaved `useId`/`useState` order roundtrip through Rust/TSX and execute in persistent Native/Web documents. Generated Rust uses an explicit state snapshot; captured values and functional updater parameters have distinct lifetimes. General hooks, controlled callbacks and complete Web widget rendering remain incomplete.

`meari-kitchen-sink` is a batch native verifier, with CPU and optional Windows `gl` paths. `screen-check REPORT.json` verifies CSS canvas sizing/media changes; `screen-check-gl` requires `--features gl`. Source-owned fixtures ship with the native package. Official documentation corpora, compiler downloads and generated evidence are separate local inputs, not public source assets.

External compiler tests require the pinned Tailwind standalone executable via `BRIDGE_TAILWIND`. Official corpus tests additionally require `MEARI_KITCHEN_SINK`. These tests are explicitly ignored by default; use `--include-ignored` only with those inputs. Hardware GL tests require an actual compatible context. Skipped tests are not compatibility evidence.

## Fonts

Every canonical face is a Noto family (Noto Sans / Serif / Sans Mono and their script variants, SIL OFL-1.1) pinned to a `google/fonts` revision with length and sha256. No font file is committed and no font bytes are embedded in any binary. `meari fonts fetch` downloads and verifies them into `$MEARI_FONT_CACHE` (else the nearest `runtime/fonts`); apps never download at run time. See [fonts](.spec/sot/fonts.sot.md).

## Cross-OS verification

The manually dispatched CI workflow runs build, clippy and tests on Linux, Windows and macOS. The PixelGate visual test compares the native renderer against headless Chrome references: Linux is the Strict baseline gate, Windows and macOS run with the Advisory policy.

## Distribution

License: MIT OR Apache-2.0. See LICENSE-MIT, LICENSE-APACHE, NOTICE and THIRD_PARTY_NOTICES.md. The public repository is [modootoday/meari](https://github.com/modootoday/meari). All workspace crates are published on [crates.io](https://crates.io/crates/meari) through the tag-triggered release workflow; see [CHANGELOG](CHANGELOG.md) for breaking changes.

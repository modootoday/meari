# Meari

> **Write once, echo everywhere.**

Meari (메아리, "echo"; Multi-platform Experience Across Renderers Interface) is a Rust UI framework and WASM-based Native+Web builder under development. Meari shares typed UI contracts across source conversion, native rendering and browser adapters. CLI tools exercise those same APIs.

## Packages

| Package | Responsibility |
| --- | --- |
| `meari` | Portable domain/application, bounded Rust/TSX conversion, shared media/HMR contracts, optional `wasm` adapter |
| `meari-native` | Native CSS/layout/paint, widget interactions, accessibility and OS adapters |
| `meari-native-tools` | Executable hosts for the native showcase/CLI and batch kitchen-sink verifier |
| `meari-builder` | Host-side project loading, registered compiler adapters, immutable app bundles and build/serve CLI |

The packages live under `packages/` in one Cargo workspace. The default member is `meari`; native dependencies are opt-in. Web adapters currently use `meari`'s `wasm` feature. A separate `meari-web` package is reserved for a future independent Web runtime, not an empty placeholder.

```text
cargo test -j1
cargo clippy -j1 --all-targets -- -D warnings
cargo test -j1 -p meari-native -- --test-threads=1
cargo check -p meari --target wasm32-unknown-unknown --features wasm
cargo run -p meari --bin meari-cli -- --help
cargo run -p meari-native-tools --bin meari-native -- --help
cargo run -p meari-builder -- --help
```

## Current support

The core exposes `parse`, `emit`, `convert`, `prepare_native` and injectable application ports. Generated Rust imports `meari::runtime`. Typed string/bool props, bounded `useId`, source mappings and diagnostics are supported. General React hooks/effects/events/control flow and arbitrary Rust/React equivalence remain incomplete.

Tailwind CSS and shadcn/ui are the default design profile. Native adapters consume compiled CSS and source-owned widget contracts. Direct custom CSS, CSS-first Tailwind configuration, semantic SVG Icon tokens, persistent input/focus and selected composite interactions are implemented. Windows has the verified native host; complete CSS/provider parity and Linux/macOS native acceptance remain unfinished.

The `wasm` feature exposes conversion APIs, browser media-query evaluation and the Rust `WebDocument` adapter for persistent DOM. A Web host supplies a container, component, typed props, CSS and a unique document scope/epoch; the adapter owns its listeners and mount lifetime. Complete Web component recipes and the Native+Web builder remain unfinished. Shared dual-target HMR coordination and retained source commit/rollback adapters exist; browser editor transport and coordinated commits of both running hosts remain unfinished. See [architecture and support boundaries](.spec/sot/architecture-and-support-boundaries.sot.md) and [runtime lifecycle](.spec/sot/runtime-lifecycle-and-verification-boundaries.sot.md).

## Native verification

Primitive source `useState` declarations, scalar setter expressions and interleaved `useId`/`useState` order roundtrip through Rust/TSX and execute in persistent Native/Web documents. Generated Rust uses an explicit state snapshot; captured values and functional updater parameters have distinct lifetimes. General hooks, controlled callbacks and complete Web widget rendering remain incomplete.

`meari-kitchen-sink` is a batch native verifier, with CPU and optional Windows `gl` paths. `screen-check REPORT.json` verifies CSS canvas sizing/media changes; `screen-check-gl` requires `--features gl`. Source-owned fixtures ship with the native package. Official documentation corpora, compiler downloads and generated evidence are separate local inputs, not public source assets.

External compiler tests require the pinned Tailwind standalone executable via `BRIDGE_TAILWIND`. Official corpus tests additionally require `MEARI_KITCHEN_SINK`. These tests are explicitly ignored by default; use `--include-ignored` only with those inputs. Hardware GL tests require an actual compatible context. Skipped tests are not compatibility evidence.

## Distribution

License: MIT OR Apache-2.0. See LICENSE-MIT, LICENSE-APACHE and NOTICE. The public repository is [modootoday/meari](https://github.com/modootoday/meari). Cargo publication remains disabled while packaging and framework acceptance are incomplete.
